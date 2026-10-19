//! The command section of a `git-receive-pack` request, and the answer that refuses a push without running
//! `git http-backend`, in the pkt-line format of the Git protocol (v0 and v1; pushing has no v2).

use ferrisgit_domain::protected_branch::{RefRefusal, RefUpdate};

/// The ref updates a push asks for, and the capabilities the client announced with the first one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivePackCommands {
    pub updates: Vec<RefUpdate>,
    pub capabilities: Vec<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReceivePackParseError {
    #[error("truncated or malformed pkt-line")]
    Malformed,
    #[error("signed pushes are not supported")]
    PushCertificate,
    #[error("malformed ref update command")]
    BadCommand,
}

/// Reads the pkt-lines up to the first flush packet: optional `shallow` lines, then one `<old> <new> <ref>` command per
/// line, the first one carrying the capabilities after a NUL byte. The pack that follows is not looked at.
pub fn parse_commands(body: &[u8]) -> Result<ReceivePackCommands, ReceivePackParseError> {
    let mut rest = body;
    let mut updates = Vec::new();
    let mut capabilities = Vec::new();
    loop {
        let Some(length) = rest.get(..4) else {
            return Err(ReceivePackParseError::Malformed);
        };
        let length = std::str::from_utf8(length)
            .ok()
            .and_then(|hex| usize::from_str_radix(hex, 16).ok())
            .ok_or(ReceivePackParseError::Malformed)?;
        if length == 0 {
            break;
        }
        if length < 4 || length > rest.len() {
            return Err(ReceivePackParseError::Malformed);
        }
        let payload = &rest[4..length];
        rest = &rest[length..];
        let payload = payload.strip_suffix(b"\n").unwrap_or(payload);
        let (line, announced) = match payload.iter().position(|&b| b == 0) {
            Some(nul) => (&payload[..nul], Some(&payload[nul + 1..])),
            None => (payload, None),
        };
        if line.starts_with(b"push-cert") {
            return Err(ReceivePackParseError::PushCertificate);
        }
        if line.starts_with(b"shallow ") {
            continue;
        }
        let line = std::str::from_utf8(line).map_err(|_| ReceivePackParseError::BadCommand)?;
        let mut fields = line.splitn(3, ' ');
        let (Some(old_sha), Some(new_sha), Some(ref_name)) =
            (fields.next(), fields.next(), fields.next())
        else {
            return Err(ReceivePackParseError::BadCommand);
        };
        let is_object_id =
            |sha: &str| matches!(sha.len(), 40 | 64) && sha.bytes().all(|b| b.is_ascii_hexdigit());
        if !is_object_id(old_sha) || !is_object_id(new_sha) || ref_name.is_empty() {
            return Err(ReceivePackParseError::BadCommand);
        }
        if updates.is_empty()
            && let Some(announced) = announced
        {
            capabilities = String::from_utf8_lossy(announced)
                .split(' ')
                .filter(|c| !c.is_empty())
                .map(str::to_string)
                .collect();
        }
        updates.push(RefUpdate {
            old_sha: old_sha.to_string(),
            new_sha: new_sha.to_string(),
            ref_name: ref_name.to_string(),
        });
    }
    if updates.is_empty() {
        return Err(ReceivePackParseError::BadCommand);
    }
    Ok(ReceivePackCommands {
        updates,
        capabilities,
    })
}

impl ReceivePackCommands {
    fn has(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|c| c == capability)
    }

    /// Whether the client asked for a status per ref, the only way to tell it why a ref was refused.
    pub fn wants_report_status(&self) -> bool {
        self.has("report-status") || self.has("report-status-v2")
    }

    /// The largest side-band packet the client accepts, `None` without side-band.
    fn side_band_packet_limit(&self) -> Option<usize> {
        if self.has("side-band-64k") {
            Some(65520)
        } else if self.has("side-band") {
            Some(1000)
        } else {
            None
        }
    }
}

const FLUSH: &[u8] = b"0000";

fn pkt_line(payload: &[u8]) -> Vec<u8> {
    let mut packet = format!("{:04x}", payload.len() + 4).into_bytes();
    packet.extend_from_slice(payload);
    packet
}

/// One line of text the reason carries: a newline would end the status line early.
fn single_line(text: &str) -> String {
    text.replace(['\n', '\r'], " ")
}

/// The answer to a push refused as a whole: every command gets `ng`, the refused ones with their reason and the others
/// because the push is all or nothing, and the reasons are also sent as `remote:` messages when the client reads a
/// side band. Only meaningful when [`ReceivePackCommands::wants_report_status`] is true.
pub fn refusal_response(commands: &ReceivePackCommands, refusals: &[RefRefusal]) -> Vec<u8> {
    let mut report = pkt_line(b"unpack ok\n");
    for update in &commands.updates {
        let reason = refusals
            .iter()
            .find(|refusal| refusal.ref_name == update.ref_name)
            .map(|refusal| single_line(&refusal.reason))
            .unwrap_or_else(|| "push declined: it also updates a protected branch".to_string());
        report.extend(pkt_line(
            format!("ng {} {reason}\n", update.ref_name).as_bytes(),
        ));
    }
    report.extend_from_slice(FLUSH);

    let Some(limit) = commands.side_band_packet_limit() else {
        return report;
    };
    let mut response = Vec::new();
    for refusal in refusals {
        let message = format!("FerrisGit: {}\n", single_line(&refusal.reason));
        response.extend(side_band(2, message.as_bytes(), limit));
    }
    response.extend(side_band(1, &report, limit));
    response.extend_from_slice(FLUSH);
    response
}

/// `data` cut into side-band packets of channel `band` (1 carries the protocol, 2 progress text shown as `remote:`).
fn side_band(band: u8, data: &[u8], packet_limit: usize) -> Vec<u8> {
    let chunk_size = packet_limit - 5;
    data.chunks(chunk_size)
        .flat_map(|chunk| {
            let mut payload = vec![band];
            payload.extend_from_slice(chunk);
            pkt_line(&payload)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZERO: &str = "0000000000000000000000000000000000000000";
    const A: &str = "1111111111111111111111111111111111111111";
    const B: &str = "2222222222222222222222222222222222222222";

    fn request(lines: &[&str]) -> Vec<u8> {
        let mut body: Vec<u8> = lines.iter().flat_map(|l| pkt_line(l.as_bytes())).collect();
        body.extend_from_slice(FLUSH);
        body.extend_from_slice(b"PACK\x00\x00\x00\x02");
        body
    }

    #[test]
    fn the_commands_and_the_capabilities_of_the_first_one_are_read() {
        let body = request(&[
            &format!("{A} {B} refs/heads/main\0 report-status side-band-64k agent=git/2.47\n"),
            &format!("{ZERO} {A} refs/heads/feature\n"),
        ]);
        let commands = parse_commands(&body).unwrap();
        assert_eq!(
            commands.updates,
            vec![
                RefUpdate {
                    old_sha: A.to_string(),
                    new_sha: B.to_string(),
                    ref_name: "refs/heads/main".to_string(),
                },
                RefUpdate {
                    old_sha: ZERO.to_string(),
                    new_sha: A.to_string(),
                    ref_name: "refs/heads/feature".to_string(),
                },
            ]
        );
        assert_eq!(
            commands.capabilities,
            vec!["report-status", "side-band-64k", "agent=git/2.47"]
        );
        assert!(commands.wants_report_status());
    }

    #[test]
    fn shallow_lines_are_skipped() {
        let body = request(&[
            &format!("shallow {A}\n"),
            &format!("{A} {B} refs/heads/main\0report-status\n"),
        ]);
        assert_eq!(parse_commands(&body).unwrap().updates.len(), 1);
    }

    #[test]
    fn a_malformed_request_is_an_error_not_an_empty_push() {
        assert_eq!(parse_commands(b""), Err(ReceivePackParseError::Malformed));
        assert_eq!(parse_commands(b"00"), Err(ReceivePackParseError::Malformed));
        assert_eq!(
            parse_commands(b"zzzz"),
            Err(ReceivePackParseError::Malformed)
        );
        assert_eq!(
            parse_commands(b"0fff short"),
            Err(ReceivePackParseError::Malformed)
        );
        assert_eq!(
            parse_commands(FLUSH),
            Err(ReceivePackParseError::BadCommand)
        );
        assert_eq!(
            parse_commands(&request(&["not a command\n"])),
            Err(ReceivePackParseError::BadCommand)
        );
        assert_eq!(
            parse_commands(&request(&[&format!("{A} {B}\n")])),
            Err(ReceivePackParseError::BadCommand)
        );
        assert_eq!(
            parse_commands(&request(&["push-cert\0report-status\n"])),
            Err(ReceivePackParseError::PushCertificate)
        );
    }

    #[test]
    fn without_side_band_the_report_is_plain_pkt_lines() {
        let commands = parse_commands(&request(&[
            &format!("{A} {B} refs/heads/main\0report-status\n"),
            &format!("{A} {B} refs/heads/feature\n"),
        ]))
        .unwrap();
        let response = refusal_response(
            &commands,
            &[RefRefusal {
                ref_name: "refs/heads/main".to_string(),
                reason: "branch 'main' is protected".to_string(),
            }],
        );
        let expected = [
            pkt_line(b"unpack ok\n"),
            pkt_line(b"ng refs/heads/main branch 'main' is protected\n"),
            pkt_line(b"ng refs/heads/feature push declined: it also updates a protected branch\n"),
            FLUSH.to_vec(),
        ]
        .concat();
        assert_eq!(response, expected);
    }

    #[test]
    fn with_side_band_the_report_goes_on_band_one_after_the_messages_on_band_two() {
        let commands = parse_commands(&request(&[&format!(
            "{A} {B} refs/heads/main\0report-status side-band-64k\n"
        )]))
        .unwrap();
        let refusal = RefRefusal {
            ref_name: "refs/heads/main".to_string(),
            reason: "protected\nbranch".to_string(),
        };
        let response = refusal_response(&commands, &[refusal]);

        let mut report = pkt_line(b"unpack ok\n");
        report.extend(pkt_line(b"ng refs/heads/main protected branch\n"));
        report.extend_from_slice(FLUSH);
        let expected = [
            pkt_line(b"\x02FerrisGit: protected branch\n"),
            pkt_line(&[&[1u8][..], &report].concat()),
            FLUSH.to_vec(),
        ]
        .concat();
        assert_eq!(response, expected);
    }

    #[test]
    fn a_long_side_band_payload_is_split_under_the_packet_limit() {
        let packets = side_band(1, &[b'x'; 2500], 1000);
        let mut rest = packets.as_slice();
        let mut total = 0;
        while !rest.is_empty() {
            let length =
                usize::from_str_radix(std::str::from_utf8(&rest[..4]).unwrap(), 16).unwrap();
            assert!(length <= 1000);
            assert_eq!(rest[4], 1);
            total += length - 5;
            rest = &rest[length..];
        }
        assert_eq!(total, 2500);
    }
}
