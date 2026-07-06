//! Standalone example: USB CDC-NCM web server on RP2350.
//!
//! Plug the device into a Windows 10 (1903+) or Linux PC via USB.
//! The device appears as a USB Ethernet adapter (no drivers needed).
//! A DHCP server assigns the host an IP automatically.
//! Open a browser at http://helicopter.local/ (or http://192.168.7.1/).
//!
//! The web UI lets you:
//!   - See live uptime
//!   - Change the LED blink rate with a slider
//!   - Rename the mDNS friendly name (e.g. "mydevice" → mydevice.local)
//!
//! The unique per-chip name (helicopter-XXXXXX.local) always works regardless.
//!
//! Run with: cargo run --example usb_web_server

#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use core::fmt::Write as FmtWrite;
use core::sync::atomic::{AtomicU32, Ordering};

use defmt::info;
use embassy_executor::Spawner;
use embassy_net::{
    Config as NetConfig, IpAddress, IpEndpoint, Ipv4Address, Ipv4Cidr, Stack, StackResources,
    StaticConfigV4,
};
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{TRNG, USB};
use embassy_rp::trng::{Config as TrngConfig, InterruptHandler as TrngIrq, Trng};
use embassy_rp::usb::{Driver as UsbDriver, InterruptHandler as UsbIrq};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Timer};
use embassy_usb::class::cdc_ncm::embassy_net::{
    Device as NcmDevice, Runner as NcmRunner, State as NcmNetState,
};
use embassy_usb::class::cdc_ncm::{CdcNcmClass, State as NcmState};
use embassy_usb::{Builder as UsbBuilder, Config as UsbConfig};
use heapless::String as HString;
use picoserve::response::{Content, IntoResponse};
use picoserve::routing;
use picoserve::{Config as ServeConfig, Router, Timeouts};
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => UsbIrq<USB>;
    TRNG_IRQ    => TrngIrq<TRNG>;
});

// ── Constants ─────────────────────────────────────────────────────────────────

const MTU: usize = 1514;
const DEVICE_IP: [u8; 4] = [192, 168, 7, 1];

// ── Shared state (accessed from multiple tasks) ────────────────────────────────

/// LED blink interval in milliseconds.
static BLINK_MS: AtomicU32 = AtomicU32::new(500);

/// User-settable friendly mDNS hostname (without ".local").
static FRIENDLY_NAME: Mutex<CriticalSectionRawMutex, HString<64>> =
    Mutex::new(HString::new());

// ── Static allocations ─────────────────────────────────────────────────────────

static NCM_STATE: StaticCell<NcmState> = StaticCell::new();
static NCM_NET_STATE: StaticCell<NcmNetState<MTU, 4, 4>> = StaticCell::new();
static NET_RESOURCES: StaticCell<StackResources<5>> = StaticCell::new();

type MyUsb = UsbDriver<'static, USB>;

// ── Embassy tasks ──────────────────────────────────────────────────────────────

#[embassy_executor::task]
async fn usb_task(mut device: embassy_usb::UsbDevice<'static, MyUsb>) -> ! {
    device.run().await
}

#[embassy_executor::task]
async fn ncm_task(runner: NcmRunner<'static, MyUsb, MTU>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, NcmDevice<'static, MTU>>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn dhcp_task(stack: Stack<'static>) -> ! {
    use core::net::Ipv4Addr;
    let config = leasehund::DhcpConfigBuilder::<1>::new()
        .server_ip(Ipv4Addr::from(DEVICE_IP))
        .subnet_mask(Ipv4Addr::new(255, 255, 255, 0))
        .no_router() // host keeps its existing default route
        .ip_pool(
            Ipv4Addr::new(DEVICE_IP[0], DEVICE_IP[1], DEVICE_IP[2], 100),
            Ipv4Addr::new(DEVICE_IP[0], DEVICE_IP[1], DEVICE_IP[2], 199),
        )
        .build();
    let mut server = leasehund::DhcpServer::<8, 1>::with_config(config);
    // Log every completed lease/release so the RTT log shows the DHCP
    // handshake handing out an address.
    server
        .run_with_callback(stack, |event| {
            info!("DHCP transaction: {}", event);
        })
        .await
}

/// Signalled with the previous friendly name whenever it changes, so
/// `mdns_task` can send a goodbye for the old name and announce the new one
/// immediately instead of waiting for listeners' caches to expire (TTL=120s).
static NAME_CHANGED: Signal<CriticalSectionRawMutex, HString<64>> = Signal::new();

#[embassy_executor::task]
async fn mdns_task(stack: Stack<'static>, unique_suffix: [u8; 3]) -> ! {
    use embassy_futures::select::{select, Either};
    use embassy_net::udp::{PacketMetadata, UdpSocket};

    let mcast = IpAddress::v4(224, 0, 0, 251);

    stack.wait_config_up().await;
    stack.join_multicast_group(mcast).ok();

    static RX_META: StaticCell<[PacketMetadata; 4]> = StaticCell::new();
    static TX_META: StaticCell<[PacketMetadata; 4]> = StaticCell::new();
    static RX_BUF: StaticCell<[u8; 1024]> = StaticCell::new();
    static TX_BUF: StaticCell<[u8; 1024]> = StaticCell::new();

    let mut socket = UdpSocket::new(
        stack,
        RX_META.init([PacketMetadata::EMPTY; 4]),
        RX_BUF.init([0u8; 1024]),
        TX_META.init([PacketMetadata::EMPTY; 4]),
        TX_BUF.init([0u8; 1024]),
    );
    socket.bind(5353).ok();

    let mut pkt = [0u8; 512];
    let mut resp = [0u8; 512];

    let mut unique_name = HString::<32>::new();
    write!(
        &mut unique_name,
        "helicopter-{:02x}{:02x}{:02x}",
        unique_suffix[0], unique_suffix[1], unique_suffix[2]
    )
    .ok();
    let mut unique_enc = [0u8; 80];
    let unique_len = encode_mdns_name(&mut unique_enc, unique_name.as_str(), "local");

    loop {
        match select(socket.recv_from(&mut pkt), NAME_CHANGED.wait()).await {
            Either::First(Ok((n, _src))) => {
                let mut friendly_enc = [0u8; 80];
                let friendly_len = {
                    let name = FRIENDLY_NAME.lock().await;
                    encode_mdns_name(&mut friendly_enc, name.as_str(), "local")
                };

                if let Some(resp_len) = handle_mdns_query(
                    &pkt[..n],
                    &mut resp,
                    DEVICE_IP,
                    &friendly_enc[..friendly_len],
                    &unique_enc[..unique_len],
                ) {
                    socket
                        .send_to(&resp[..resp_len], IpEndpoint::new(mcast, 5353))
                        .await
                        .ok();
                }
            }
            Either::First(Err(_)) => {}
            Either::Second(old_name) => {
                let mut old_enc = [0u8; 80];
                let old_len = encode_mdns_name(&mut old_enc, old_name.as_str(), "local");
                let goodbye_len =
                    build_mdns_announcement(&mut resp, &old_enc[..old_len], DEVICE_IP, 0);
                socket
                    .send_to(&resp[..goodbye_len], IpEndpoint::new(mcast, 5353))
                    .await
                    .ok();

                let mut friendly_enc = [0u8; 80];
                let friendly_len = {
                    let name = FRIENDLY_NAME.lock().await;
                    encode_mdns_name(&mut friendly_enc, name.as_str(), "local")
                };
                let announce_len = build_mdns_announcement(
                    &mut resp,
                    &friendly_enc[..friendly_len],
                    DEVICE_IP,
                    120,
                );
                socket
                    .send_to(&resp[..announce_len], IpEndpoint::new(mcast, 5353))
                    .await
                    .ok();
            }
        }
    }
}

use helicopter_collective::mdns::{build_mdns_announcement, encode_mdns_name, handle_mdns_query};

// ── HTTP ───────────────────────────────────────────────────────────────────────

/// Custom Content type that sets Content-Type: application/json.
struct JsonBody<const N: usize>([u8; N], usize);

impl<const N: usize> Content for JsonBody<N> {
    fn content_type(&self) -> &'static str {
        "application/json"
    }

    fn content_length(&self) -> usize {
        self.1
    }

    async fn write_content<W: picoserve::io::Write>(self, mut writer: W) -> Result<(), W::Error> {
        writer.write_all(&self.0[..self.1]).await
    }
}

struct SetBlinkHandler;

impl picoserve::routing::RequestHandlerService for SetBlinkHandler {
    async fn call_request_handler_service<R: picoserve::io::Read, W: picoserve::response::ResponseWriter<Error = R::Error>>(
        &self,
        _state: &(),
        _: (),
        mut request: picoserve::request::Request<'_, R>,
        response_writer: W,
    ) -> Result<picoserve::ResponseSent, W::Error> {
        let ms = request
            .body_connection
            .body()
            .read_all()
            .await
            .ok()
            .and_then(|b| core::str::from_utf8(b).ok())
            .and_then(|s| s.trim().parse::<u32>().ok());
        if let Some(ms) = ms {
            BLINK_MS.store(ms.clamp(50, 2000), Ordering::Relaxed);
        }
        "ok".write_to(request.body_connection.finalize().await?, response_writer).await
    }
}

struct SetNameHandler;

impl picoserve::routing::RequestHandlerService for SetNameHandler {
    async fn call_request_handler_service<R: picoserve::io::Read, W: picoserve::response::ResponseWriter<Error = R::Error>>(
        &self,
        _state: &(),
        _: (),
        mut request: picoserve::request::Request<'_, R>,
        response_writer: W,
    ) -> Result<picoserve::ResponseSent, W::Error> {
        let trimmed = request
            .body_connection
            .body()
            .read_all()
            .await
            .ok()
            .and_then(|b| core::str::from_utf8(b).ok())
            .map(|s| {
                let mut t = HString::<64>::new();
                t.push_str(s.trim()).ok();
                t
            });
        if let Some(name) = trimmed {
            if !name.is_empty() {
                let mut n = FRIENDLY_NAME.lock().await;
                if n.as_str() != name.as_str() {
                    let old = n.clone();
                    n.clear();
                    n.push_str(name.as_str()).ok();
                    NAME_CHANGED.signal(old);
                }
            }
        }
        "ok".write_to(request.body_connection.finalize().await?, response_writer).await
    }
}

const INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>Helicopter Collective</title>
  <style>
    body{font-family:sans-serif;max-width:520px;margin:2em auto;padding:0 1em;color:#222}
    h1{font-size:1.4em;margin-bottom:0.2em}
    .card{margin:1.2em 0;padding:1em;border:1px solid #ccc;border-radius:8px}
    label{display:block;margin:.4em 0 .15em;font-size:.9em}
    input[type=range]{width:100%}
    input[type=text]{width:100%;box-sizing:border-box;padding:.35em;border:1px solid #aaa;border-radius:4px}
    button{margin-top:.6em;padding:.4em 1em;cursor:pointer}
    #status{font-family:monospace;background:#f5f5f5;padding:.5em .7em;border-radius:4px;font-size:.9em}
    .dim{color:#777;font-size:.85em}
    .copyline{display:flex;align-items:center;gap:.5em;margin:.2em 0 .6em}
    .copyline code{font-family:monospace;background:#f5f5f5;padding:.3em .5em;border-radius:4px;user-select:all}
    .copyline button{margin:0;padding:.2em .6em;font-size:1.1em;line-height:1}
  </style>
</head>
<body>
  <h1>&#x1F681; Helicopter Collective</h1>
  <div class="card">
    <strong>Status</strong>
    <div id="status">connecting&hellip;</div>
  </div>
  <div class="card">
    <strong>LED blink rate</strong>
    <label>Interval: <span id="blink-val">500</span>&thinsp;ms</label>
    <input type="range" id="blink" min="50" max="2000" value="500"
           oninput="dirty.blink=true;document.getElementById('blink-val').textContent=this.value">
    <button onclick="setBlink()">Apply</button>
  </div>
  <div class="card">
    <strong>mDNS name</strong>
    <label>Friendly name</label>
    <div class="copyline"><code id="fname-code">&#x2026;</code><button title="Copy" aria-label="Copy" onclick="copyText('fname-code',this)">&#x2398;</button></div>
    <label class="dim">Unique name (always works)</label>
    <div class="copyline"><code id="uname-code">&#x2026;</code><button title="Copy" aria-label="Copy" onclick="copyText('uname-code',this)">&#x2398;</button></div>
    <input type="text" id="new-name" placeholder="helicopter" maxlength="63" onchange="setName()">
  </div>
  <script>
    const dirty={blink:false};
    async function refresh(){
      try{
        const d=await fetch('/api/status').then(r=>r.json());
        document.getElementById('status').textContent='Uptime: '+d.uptime_secs+'s';
        if(!dirty.blink){
          document.getElementById('blink-val').textContent=d.blink_ms;
          document.getElementById('blink').value=d.blink_ms;
        }
        document.getElementById('fname-code').textContent=d.friendly_name+'.local';
        document.getElementById('uname-code').textContent=d.unique_name+'.local';
      }catch(e){
        document.getElementById('status').textContent='offline';
      }
    }
    async function copyText(id,btn){
      const text=document.getElementById(id).textContent;
      try{
        await navigator.clipboard.writeText(text);
      }catch(e){
        const range=document.createRange();
        range.selectNodeContents(document.getElementById(id));
        const sel=window.getSelection();
        sel.removeAllRanges();
        sel.addRange(range);
        document.execCommand('copy');
        sel.removeAllRanges();
      }
      const orig=btn.textContent;
      btn.textContent='✓';
      setTimeout(()=>{btn.textContent=orig;},1200);
    }
    async function setBlink(){
      await fetch('/api/blink',{method:'POST',body:String(document.getElementById('blink').value)});
      dirty.blink=false;
    }
    async function setName(){
      const n=document.getElementById('new-name').value.trim();
      if(!n)return;
      await fetch('/api/name',{method:'POST',body:n});
      document.getElementById('new-name').value='';
      setTimeout(refresh,400);
    }
    setInterval(refresh,2000);
    refresh();
  </script>
</body>
</html>"#;

#[embassy_executor::task]
async fn http_task(stack: Stack<'static>, unique_suffix: [u8; 3]) -> ! {
    static HTTP_RX: StaticCell<[u8; 1024]> = StaticCell::new();
    static HTTP_TX: StaticCell<[u8; 1024]> = StaticCell::new();
    static HTTP_BUF: StaticCell<[u8; 2048]> = StaticCell::new();

    let tcp_rx = HTTP_RX.init([0u8; 1024]);
    let tcp_tx = HTTP_TX.init([0u8; 1024]);
    let http_buf = HTTP_BUF.init([0u8; 2048]);

    // Build the unique name string once, stored in a static so it's 'static.
    let unique_name: &'static str = {
        static UN: StaticCell<HString<32>> = StaticCell::new();
        let mut s = HString::<32>::new();
        write!(
            &mut s,
            "helicopter-{:02x}{:02x}{:02x}",
            unique_suffix[0], unique_suffix[1], unique_suffix[2]
        )
        .ok();
        UN.init(s).as_str()
    };

    let app = Router::new()
        .route(
            "/",
            routing::get_service(picoserve::response::fs::File::html(INDEX_HTML)),
        )
        .route(
            "/api/status",
            routing::get(async move || {
                let blink_ms = BLINK_MS.load(Ordering::Relaxed);
                let uptime_secs = Instant::now().as_secs();

                // Lock the name briefly to read it
                let mut buf = [0u8; 256];
                let len = {
                    let name = FRIENDLY_NAME.lock().await;
                    helicopter_collective::status::format_status_json(
                        &mut buf,
                        blink_ms,
                        uptime_secs,
                        name.as_str(),
                        unique_name,
                    )
                    .unwrap_or(0)
                }; // MutexGuard dropped here
                JsonBody(buf, len)
            }),
        )
        .route(
            "/api/blink",
            routing::post_service(SetBlinkHandler),
        )
        .route(
            "/api/name",
            routing::post_service(SetNameHandler),
        );

    let config = ServeConfig::new(Timeouts::default());

    loop {
        picoserve::Server::new(&app, &config, http_buf)
            .listen_and_serve(0u8, stack, 80, tcp_rx, tcp_tx)
            .await;
    }
}

#[embassy_executor::task]
async fn led_task(mut led: Output<'static>) -> ! {
    loop {
        let ms = BLINK_MS.load(Ordering::Relaxed);
        led.set_high();
        Timer::after(Duration::from_millis(50)).await;
        led.set_low();
        Timer::after(Duration::from_millis(ms as u64)).await;
    }
}

// ── Entry point ────────────────────────────────────────────────────────────────

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    info!("USB web server example starting");

    // Read chip unique ID from OTP for per-device names and MAC address
    let chip_id = embassy_rp::otp::get_chipid().unwrap_or(0xDEAD_BEEF_DEAD_BEEF_u64);
    let id = chip_id.to_be_bytes();
    let unique_suffix = [id[5], id[6], id[7]];

    // Two distinct MAC addresses derived from the chip ID, one for each end of
    // the USB Ethernet link. They must differ (same MAC on both ends breaks
    // delivery), and the *host*-side address must NOT have the
    // locally-administered bit (bit 1 of the first octet) set — embassy-usb's
    // CDC-NCM docs warn that some hosts (Windows in particular) won't bring the
    // link up otherwise, leaving the adapter at "Media disconnected".
    //
    // host_mac   -> the address Windows' virtual adapter uses (iMACAddress);
    //               first octet 0x00 => universally-administered, unicast.
    // device_mac -> the address this device's own network stack uses;
    //               first octet 0x02 => locally-administered, fine on this side.
    let host_mac = [0x00u8, id[0], id[1], id[2], id[3], id[4]];
    let device_mac = [0x02u8, id[0], id[1], id[2], id[3], id[4]];

    // Seed the network stack with entropy from the TRNG
    let mut trng = Trng::new(p.TRNG, Irqs, TrngConfig::default());
    let seed = trng.blocking_next_u64();

    // Set default friendly hostname
    {
        let mut name = FRIENDLY_NAME.lock().await;
        name.push_str("helicopter").ok();
    }

    // ── USB setup ──────────────────────────────────────────────────────────────

    let usb_driver = UsbDriver::new(p.USB, Irqs);

    let mut usb_config = UsbConfig::new(0x6666, 0x0001);
    usb_config.manufacturer = Some("Helicopter Collective");
    usb_config.product = Some("HC Web Example");
    usb_config.serial_number = Some("HC0001");
    // Miscellaneous/IAD composite device class. These are already the
    // embassy-usb defaults (a CDC-NCM function is described via an Interface
    // Association Descriptor), set explicitly here for clarity. Windows binds
    // its UsbNcm driver via the interface-level CDC/NCM descriptors.
    usb_config.device_class = 0xEF;
    usb_config.device_sub_class = 0x02;
    usb_config.device_protocol = 0x01;

    static CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static BOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static MSOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    // 128 bytes matches embassy's reference NCM example; the control-transfer
    // data staging buffer must be comfortably larger than the longest string
    // descriptor the host reads during enumeration.
    static CTRL_BUF: StaticCell<[u8; 128]> = StaticCell::new();

    let mut usb_builder = UsbBuilder::new(
        usb_driver,
        usb_config,
        CONFIG_DESC.init([0u8; 256]),
        BOS_DESC.init([0u8; 256]),
        MSOS_DESC.init([0u8; 256]),
        CTRL_BUF.init([0u8; 128]),
    );

    // CDC-NCM class (Ethernet over USB). The MAC passed here is the
    // iMACAddress — the address the host's virtual adapter takes on.
    let ncm = CdcNcmClass::new(
        &mut usb_builder,
        NCM_STATE.init(NcmState::new()),
        host_mac,
        64, // full-speed USB max packet size
    );

    let usb_device = usb_builder.build();

    // Convert CDC-NCM into an embassy-net Ethernet driver
    let (ncm_runner, eth_device) = ncm.into_embassy_net_device::<MTU, 4, 4>(
        NCM_NET_STATE.init(NcmNetState::new()),
        device_mac,
    );

    // ── Network stack (static IP) ──────────────────────────────────────────────

    let (stack, net_runner) = embassy_net::new(
        eth_device,
        NetConfig::ipv4_static(StaticConfigV4 {
            address: Ipv4Cidr::new(
                Ipv4Address::new(DEVICE_IP[0], DEVICE_IP[1], DEVICE_IP[2], DEVICE_IP[3]),
                24,
            ),
            gateway: None,
            dns_servers: heapless::Vec::new(),
        }),
        NET_RESOURCES.init(StackResources::new()),
        seed,
    );

    // ── Spawn all tasks ────────────────────────────────────────────────────────

    spawner.spawn(usb_task(usb_device).unwrap());
    spawner.spawn(ncm_task(ncm_runner).unwrap());
    spawner.spawn(net_task(net_runner).unwrap());
    spawner.spawn(dhcp_task(stack).unwrap());
    spawner.spawn(mdns_task(stack, unique_suffix).unwrap());
    spawner.spawn(http_task(stack, unique_suffix).unwrap());
    spawner.spawn(led_task(Output::new(p.PIN_25, Level::Low)).unwrap());
}
