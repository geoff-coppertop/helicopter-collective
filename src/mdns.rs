/// Encode `hostname.domain` into DNS wire format (length-prefixed labels + NUL).
/// Returns the number of bytes written.
pub fn encode_mdns_name(buf: &mut [u8], hostname: &str, domain: &str) -> usize {
    let mut pos = 0;
    for part in [hostname, domain] {
        let b = part.as_bytes();
        buf[pos] = b.len() as u8;
        pos += 1;
        buf[pos..pos + b.len()].copy_from_slice(b);
        pos += b.len();
    }
    buf[pos] = 0;
    pos + 1
}

/// Parse an mDNS query and build a response if it asks for one of our names.
/// Returns the response length, or None if the packet is not for us.
pub fn handle_mdns_query(
    pkt: &[u8],
    resp: &mut [u8],
    device_ip: [u8; 4],
    friendly_enc: &[u8],
    unique_enc: &[u8],
) -> Option<usize> {
    if pkt.len() < 12 {
        return None;
    }
    // Only handle queries (QR bit = 0)
    if pkt[2] & 0x80 != 0 {
        return None;
    }

    let qdcount = u16::from_be_bytes([pkt[4], pkt[5]]) as usize;
    let mut pos = 12;
    let mut matched: Option<&[u8]> = None;

    'questions: for _ in 0..qdcount {
        let name_start = pos;
        // Skip the DNS name
        loop {
            if pos >= pkt.len() {
                return None;
            }
            let label_len = pkt[pos];
            if label_len == 0 {
                pos += 1;
                break;
            }
            if label_len & 0xC0 == 0xC0 {
                // Compression pointer
                pos += 2;
                break;
            }
            pos += 1 + label_len as usize;
        }
        if pos + 4 > pkt.len() {
            return None;
        }
        let qtype = u16::from_be_bytes([pkt[pos], pkt[pos + 1]]);
        pos += 4; // skip QTYPE + QCLASS

        // Only respond to A (1) or ANY (255) queries
        if qtype != 1 && qtype != 255 {
            continue;
        }

        let q_name = &pkt[name_start..];
        if names_equal_prefix(q_name, friendly_enc) {
            matched = Some(friendly_enc);
            break 'questions;
        }
        if names_equal_prefix(q_name, unique_enc) {
            matched = Some(unique_enc);
            break 'questions;
        }
    }

    let answer_name = matched?;
    let msg_id = u16::from_be_bytes([pkt[0], pkt[1]]);
    Some(build_a_record_answer(resp, msg_id, answer_name, device_ip, 120))
}

/// Build an mDNS A-record answer packet (header + single answer) for `name_enc`.
/// Returns the number of bytes written. `msg_id` should match the query being
/// answered, or be `0` for an unsolicited announcement/goodbye (RFC 6762 §18.1).
fn build_a_record_answer(
    resp: &mut [u8],
    msg_id: u16,
    name_enc: &[u8],
    device_ip: [u8; 4],
    ttl: u32,
) -> usize {
    let mut p = 0usize;

    // Header
    resp[p..p + 2].copy_from_slice(&msg_id.to_be_bytes());
    p += 2;
    resp[p..p + 2].copy_from_slice(&0x8400u16.to_be_bytes()); // QR=1, AA=1
    p += 2;
    resp[p..p + 2].copy_from_slice(&0u16.to_be_bytes()); // QDCOUNT=0
    p += 2;
    resp[p..p + 2].copy_from_slice(&1u16.to_be_bytes()); // ANCOUNT=1
    p += 2;
    resp[p..p + 2].copy_from_slice(&0u16.to_be_bytes()); // NSCOUNT=0
    p += 2;
    resp[p..p + 2].copy_from_slice(&0u16.to_be_bytes()); // ARCOUNT=0
    p += 2;

    // Answer record
    resp[p..p + name_enc.len()].copy_from_slice(name_enc); // NAME
    p += name_enc.len();
    resp[p..p + 2].copy_from_slice(&1u16.to_be_bytes()); // TYPE=A
    p += 2;
    resp[p..p + 2].copy_from_slice(&0x8001u16.to_be_bytes()); // CLASS=IN + cache-flush
    p += 2;
    resp[p..p + 4].copy_from_slice(&ttl.to_be_bytes());
    p += 4;
    resp[p..p + 2].copy_from_slice(&4u16.to_be_bytes()); // RDLENGTH=4
    p += 2;
    resp[p..p + 4].copy_from_slice(&device_ip); // RDATA = IP
    p += 4;

    p
}

/// Build an unsolicited mDNS announcement or goodbye packet for `name_enc`,
/// per RFC 6762 §8.3 (announcements) and §10.1 (goodbye packets).
///
/// Use `ttl = 0` as a goodbye for a name that's no longer valid (e.g. after a
/// rename), or the normal TTL (matching [`handle_mdns_query`]'s responses, 120)
/// to announce a name so listeners' caches update without waiting for expiry.
/// Returns the number of bytes written.
pub fn build_mdns_announcement(resp: &mut [u8], name_enc: &[u8], device_ip: [u8; 4], ttl: u32) -> usize {
    build_a_record_answer(resp, 0, name_enc, device_ip, ttl)
}

/// Case-insensitive comparison of the leading bytes of `query` with `encoded`.
pub fn names_equal_prefix(query: &[u8], encoded: &[u8]) -> bool {
    query.len() >= encoded.len()
        && query[..encoded.len()]
            .iter()
            .zip(encoded.iter())
            .all(|(a, b)| a.to_ascii_lowercase() == b.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── encode_mdns_name ──────────────────────────────────────────────────────

    #[test]
    fn encode_produces_length_prefixed_labels_with_nul() {
        let mut buf = [0u8; 32];
        let n = encode_mdns_name(&mut buf, "helicopter", "local");
        // \x0a h e l i c o p t e r \x05 l o c a l \x00
        assert_eq!(&buf[..n], b"\x0ahelicopter\x05local\x00");
    }

    #[test]
    fn encode_returns_correct_length() {
        let mut buf = [0u8; 32];
        let n = encode_mdns_name(&mut buf, "helicopter", "local");
        // 1 + 10 + 1 + 5 + 1 = 18
        assert_eq!(n, 18);
    }

    #[test]
    fn encode_single_char_hostname() {
        let mut buf = [0u8; 16];
        let n = encode_mdns_name(&mut buf, "a", "local");
        assert_eq!(&buf[..n], b"\x01a\x05local\x00");
        assert_eq!(n, 9); // 1 + 1 + 1 + 5 + 1
    }

    #[test]
    fn encode_unique_suffix_style() {
        let mut buf = [0u8; 32];
        let n = encode_mdns_name(&mut buf, "helicopter-a1b2c3", "local");
        assert_eq!(buf[0], 17); // len("helicopter-a1b2c3")
        assert_eq!(&buf[1..18], b"helicopter-a1b2c3");
        assert_eq!(buf[18], 5); // len("local")
        assert_eq!(&buf[19..24], b"local");
        assert_eq!(buf[24], 0);
        assert_eq!(n, 25);
    }

    // ── names_equal_prefix ───────────────────────────────────────────────────

    #[test]
    fn names_equal_exact_match() {
        let enc = b"\x0ahelicopter\x05local\x00";
        assert!(names_equal_prefix(enc, enc));
    }

    #[test]
    fn names_equal_query_longer_than_encoded() {
        let enc = b"\x0ahelicopter\x05local\x00";
        let query = b"\x0ahelicopter\x05local\x00extra_garbage";
        assert!(names_equal_prefix(query, enc));
    }

    #[test]
    fn names_equal_case_insensitive() {
        let enc = b"\x0ahelicopter\x05local\x00";
        let query = b"\x0aHELICOPTER\x05LOCAL\x00";
        assert!(names_equal_prefix(query, enc));
    }

    #[test]
    fn names_equal_query_shorter_than_encoded() {
        let enc = b"\x0ahelicopter\x05local\x00";
        let query = b"\x05short\x05local\x00";
        assert!(!names_equal_prefix(query, enc));
    }

    #[test]
    fn names_equal_different_hostname() {
        let enc = b"\x0ahelicopter\x05local\x00";
        let query = b"\x09something\x05local\x00";
        assert!(!names_equal_prefix(query, enc));
    }

    #[test]
    fn names_equal_empty_query() {
        let enc = b"\x0ahelicopter\x05local\x00";
        assert!(!names_equal_prefix(b"", enc));
    }

    // ── handle_mdns_query ────────────────────────────────────────────────────

    const DEVICE_IP: [u8; 4] = [192, 168, 7, 1];

    fn friendly_enc() -> Vec<u8> {
        let mut buf = vec![0u8; 32];
        let n = encode_mdns_name(&mut buf, "helicopter", "local");
        buf.truncate(n);
        buf
    }

    fn unique_enc() -> Vec<u8> {
        let mut buf = vec![0u8; 32];
        let n = encode_mdns_name(&mut buf, "helicopter-a1b2c3", "local");
        buf.truncate(n);
        buf
    }

    /// Build a minimal mDNS query packet for the given encoded name and QTYPE.
    fn make_query(id: u16, encoded_name: &[u8], qtype: u16) -> Vec<u8> {
        let mut pkt = Vec::new();
        pkt.extend_from_slice(&id.to_be_bytes()); // ID
        pkt.extend_from_slice(&0u16.to_be_bytes()); // flags = query
        pkt.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT = 1
        pkt.extend_from_slice(&0u16.to_be_bytes()); // ANCOUNT
        pkt.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT
        pkt.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT
        pkt.extend_from_slice(encoded_name); // QNAME
        pkt.extend_from_slice(&qtype.to_be_bytes()); // QTYPE
        pkt.extend_from_slice(&1u16.to_be_bytes()); // QCLASS = IN
        pkt
    }

    fn call(pkt: &[u8]) -> Option<Vec<u8>> {
        let fe = friendly_enc();
        let ue = unique_enc();
        let mut resp = vec![0u8; 512];
        let len = handle_mdns_query(pkt, &mut resp, DEVICE_IP, &fe, &ue)?;
        resp.truncate(len);
        Some(resp)
    }

    #[test]
    fn rejects_packet_too_short() {
        assert!(call(&[0u8; 11]).is_none());
    }

    #[test]
    fn rejects_response_packet() {
        let mut pkt = make_query(1, &friendly_enc(), 1);
        pkt[2] = 0x80; // set QR bit
        assert!(call(&pkt).is_none());
    }

    #[test]
    fn rejects_unknown_name() {
        let mut buf = [0u8; 32];
        let n = encode_mdns_name(&mut buf, "unknown", "local");
        let pkt = make_query(1, &buf[..n], 1);
        assert!(call(&pkt).is_none());
    }

    #[test]
    fn rejects_aaaa_query_type() {
        let pkt = make_query(1, &friendly_enc(), 28 /* AAAA */);
        assert!(call(&pkt).is_none());
    }

    #[test]
    fn responds_to_a_query_for_friendly_name() {
        let pkt = make_query(0xABCD, &friendly_enc(), 1 /* A */);
        let resp = call(&pkt).expect("should respond");
        // Header: ID preserved, flags = QR+AA, no questions, 1 answer
        assert_eq!(u16::from_be_bytes([resp[0], resp[1]]), 0xABCD);
        assert_eq!(u16::from_be_bytes([resp[2], resp[3]]), 0x8400);
        assert_eq!(u16::from_be_bytes([resp[4], resp[5]]), 0); // QDCOUNT=0
        assert_eq!(u16::from_be_bytes([resp[6], resp[7]]), 1); // ANCOUNT=1
    }

    #[test]
    fn responds_to_any_query_type() {
        let pkt = make_query(1, &friendly_enc(), 255 /* ANY */);
        assert!(call(&pkt).is_some());
    }

    #[test]
    fn responds_to_unique_name() {
        let pkt = make_query(1, &unique_enc(), 1);
        assert!(call(&pkt).is_some());
    }

    #[test]
    fn response_contains_correct_ip() {
        let pkt = make_query(1, &friendly_enc(), 1);
        let resp = call(&pkt).unwrap();
        let fe = friendly_enc();
        // IP starts at: 12 header + name + 2 TYPE + 2 CLASS + 4 TTL + 2 RDLEN = name_end + 10
        let ip_offset = 12 + fe.len() + 10;
        assert_eq!(&resp[ip_offset..ip_offset + 4], &DEVICE_IP);
    }

    #[test]
    fn response_contains_correct_ttl_and_type() {
        let pkt = make_query(1, &friendly_enc(), 1);
        let resp = call(&pkt).unwrap();
        let fe = friendly_enc();
        let name_end = 12 + fe.len();
        assert_eq!(u16::from_be_bytes([resp[name_end], resp[name_end + 1]]), 1); // TYPE=A
        assert_eq!(
            u16::from_be_bytes([resp[name_end + 2], resp[name_end + 3]]),
            0x8001 // CLASS=IN + cache-flush
        );
        assert_eq!(
            u32::from_be_bytes([
                resp[name_end + 4],
                resp[name_end + 5],
                resp[name_end + 6],
                resp[name_end + 7]
            ]),
            120 // TTL
        );
        assert_eq!(
            u16::from_be_bytes([resp[name_end + 8], resp[name_end + 9]]),
            4 // RDLENGTH
        );
    }

    #[test]
    fn handles_compression_pointer_in_name() {
        // Build a packet where the QNAME uses a compression pointer (0xC0 0x0C).
        // The query won't match our names (pointer doesn't resolve here),
        // but the parser must not panic or loop.
        let mut pkt = Vec::new();
        pkt.extend_from_slice(&1u16.to_be_bytes()); // ID
        pkt.extend_from_slice(&0u16.to_be_bytes()); // flags
        pkt.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT=1
        pkt.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // AN/NS/AR counts
        pkt.push(0xC0); // compression pointer high byte
        pkt.push(0x0C); // compression pointer low byte (points to offset 12 = self)
        pkt.extend_from_slice(&1u16.to_be_bytes()); // QTYPE=A
        pkt.extend_from_slice(&1u16.to_be_bytes()); // QCLASS=IN
        // Should return None (no name match) without panicking
        assert!(call(&pkt).is_none());
    }

    #[test]
    fn handles_multiple_questions_matches_second() {
        let mut buf_unknown = [0u8; 32];
        let n = encode_mdns_name(&mut buf_unknown, "other", "local");
        let unknown = &buf_unknown[..n];

        // Two questions: first is unknown, second matches friendly name
        let fe = friendly_enc();
        let mut pkt = Vec::new();
        pkt.extend_from_slice(&1u16.to_be_bytes()); // ID
        pkt.extend_from_slice(&0u16.to_be_bytes()); // flags
        pkt.extend_from_slice(&2u16.to_be_bytes()); // QDCOUNT=2
        pkt.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        pkt.extend_from_slice(unknown);
        pkt.extend_from_slice(&1u16.to_be_bytes()); // QTYPE=A
        pkt.extend_from_slice(&1u16.to_be_bytes()); // QCLASS=IN
        pkt.extend_from_slice(&fe);
        pkt.extend_from_slice(&1u16.to_be_bytes());
        pkt.extend_from_slice(&1u16.to_be_bytes());

        assert!(call(&pkt).is_some());
    }

    // ── build_mdns_announcement ──────────────────────────────────────────────

    #[test]
    fn announcement_uses_message_id_zero() {
        // RFC 6762 §18.1: unsolicited responses must use ID 0, so resolvers
        // don't mistake them for a reply correlated to an outstanding query.
        let fe = friendly_enc();
        let mut resp = vec![0u8; 512];
        build_mdns_announcement(&mut resp, &fe, DEVICE_IP, 120);
        assert_eq!(u16::from_be_bytes([resp[0], resp[1]]), 0);
    }

    #[test]
    fn announcement_sets_qr_and_aa_flags() {
        let fe = friendly_enc();
        let mut resp = vec![0u8; 512];
        build_mdns_announcement(&mut resp, &fe, DEVICE_IP, 120);
        assert_eq!(u16::from_be_bytes([resp[2], resp[3]]), 0x8400);
    }

    #[test]
    fn goodbye_packet_has_ttl_zero() {
        // A stale-cache "goodbye" only invalidates listeners' cached entry if
        // the TTL field is actually 0 — a wrong constant here would silently
        // leave the old name resolvable for the full normal TTL instead.
        let fe = friendly_enc();
        let mut resp = vec![0u8; 512];
        build_mdns_announcement(&mut resp, &fe, DEVICE_IP, 0);
        let ttl_offset = 12 + fe.len() + 4; // header + name + TYPE + CLASS
        assert_eq!(
            u32::from_be_bytes([
                resp[ttl_offset],
                resp[ttl_offset + 1],
                resp[ttl_offset + 2],
                resp[ttl_offset + 3]
            ]),
            0
        );
    }

    #[test]
    fn announcement_carries_correct_ip_and_ancount() {
        let fe = friendly_enc();
        let mut resp = vec![0u8; 512];
        let n = build_mdns_announcement(&mut resp, &fe, DEVICE_IP, 120);
        assert_eq!(u16::from_be_bytes([resp[6], resp[7]]), 1); // ANCOUNT=1
        let ip_offset = 12 + fe.len() + 10;
        assert_eq!(&resp[ip_offset..ip_offset + 4], &DEVICE_IP);
        assert_eq!(n, ip_offset + 4);
    }
}
