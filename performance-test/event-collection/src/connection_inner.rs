use x11rb::errors::ParseError;
use x11rb::protocol::{xproto, Event, Reply, Request};
use x11rb::x11_utils::{
    parse_request_header, BigRequests, ExtInfoProvider, ExtensionInformation, ReplyParsingFunction,
    TryParse, X11Error,
};

use event_lib::{serialize_message, MessageKind, MessageMetadata};
use std::collections::VecDeque;
use std::convert::TryInto;
use std::fs::File;
use std::io::Write;

/// Parse some data and print the resulting object.
///
/// The result of parsing is returned, but output is already generated on both success and error.
fn print_parse_return<T: TryParse + std::fmt::Debug>(data: &[u8]) -> Result<T, ParseError> {
    match T::try_parse(data) {
        Err(e) => {
            println!("Error while parsing: {:?}", e);
            Err(e)
        }
        Ok((obj, _remaining)) => {
            println!("{:?}", obj);
            Ok(obj)
        }
    }
}

/// Parse some data and print the resulting object.
fn print_parse<T: TryParse + std::fmt::Debug>(data: &[u8]) {
    let _ = print_parse_return::<T>(data);
}

/// Common state of an X11 connection
#[derive(Debug, Default)]
pub struct ConnectionInner {
    /// Information about present extensions. Entries are added when a reply to a `QueryExtension`
    /// request comes in.
    ext_info: ExtInfo,

    /// The number of requests that the client already sent.
    next_client_request: u16,

    /// Requests which were not yet answered by the X11 server.
    pending_replies: VecDeque<PendingReply>,

    recorder: Recorder,

    client_sent: usize,
    server_sent: usize,
}

impl ConnectionInner {
    /// Handle the client's SetupRequest
    pub fn client_setup(&mut self, packet: &[u8]) {
        self.client_sent += 1;
        // Check the byte order before parsing, because we cannot parse length fields with the
        // wrong byte order
        #[cfg(target_endian = "little")]
        let byte_order = 0x6c;
        #[cfg(target_endian = "big")]
        let byte_order = 0x42;
        if byte_order != packet[0] {
            eprintln!(
                "Client is unexpected byte order {:x} != {:x}, only native byte order is \
                 supported!",
                byte_order, packet[0],
            );
        }
        print_parse::<xproto::SetupRequest>(packet);
        assert_eq!(self.next_client_request, 0);
        self.record_message(
            MessageMetadata {
                kind: MessageKind::ClientSetup,
                seq: 0,
                event_code: 0,
            },
            packet,
        );
        self.next_client_request = 1;
    }

    /// Handle the server's Setup (or SetupFailed, SetupAuthenticate)
    pub fn server_setup(&mut self, packet: &[u8]) {
        self.server_sent += 1;
        match packet[0] {
            0 => print_parse::<xproto::SetupFailed>(packet),
            1 => {
                if let Ok(setup) = print_parse_return::<xproto::Setup>(packet) {
                    self.record_message(
                        MessageMetadata {
                            kind: MessageKind::ServerSetup,
                            seq: 0,
                            event_code: 0,
                        },
                        packet,
                    );
                    let expected = (11, 0);
                    let actual = (setup.protocol_major_version, setup.protocol_minor_version);
                    if expected != actual {
                        println!(
                            "Unexpected protocol version: {}.{} != {}.{}",
                            expected.0, expected.1, actual.0, actual.1,
                        );
                    }
                }
            }
            2 => {
                self.record_message(
                    MessageMetadata {
                        kind: MessageKind::ServerMessage,
                        seq: 0,
                        event_code: 0,
                    },
                    packet,
                );
                print_parse::<xproto::SetupAuthenticate>(packet)
            }
            _ => eprintln!("Unknown server setup response: {:?}", packet),
        }
    }

    /// Handle a request sent by the client
    pub fn client_request(&mut self, packet: &[u8]) {
        fn do_parse(inner: &mut ConnectionInner, packet: &[u8]) -> Result<(), ParseError> {
            let seqno = inner.next_client_request;
            inner.next_client_request = seqno.wrapping_add(1);

            let (header, remaining) = parse_request_header(packet, BigRequests::Enabled)?;
            let request = Request::parse(header, remaining, &mut Vec::new(), &inner.ext_info)?;
            inner.client_sent += 1;
            inner.record_message(
                MessageMetadata {
                    kind: MessageKind::ClientMessage,
                    seq: seqno,
                    event_code: 0,
                },
                packet,
            );
            inner.record_dbg(&format!("{request:?}"), MessageKind::ClientMessage, seqno);

            // Is this a QueryExtension?
            let queried_extension = if let Request::QueryExtension(ref request) = request {
                match String::from_utf8(request.name.to_vec()) {
                    Ok(name) => {
                        println!("Extension name: {}", name);
                        Some(name)
                    }
                    Err(e) => {
                        println!("Extension name is not utf8: {:?}", e);
                        None
                    }
                }
            } else {
                None
            };

            // Does the request have a reply? If so, remember it.
            if let Some(parser) = request.reply_parser() {
                inner.pending_replies.push_back(PendingReply {
                    seqno,
                    parser,
                    queried_extension,
                });
            }

            Ok(())
        }
        if let Err(e) = do_parse(self, packet) {
            eprintln!("Error while parsing a client request: {:?}", e);
        }
    }

    /// Handle an X11 error sent by the server
    pub fn server_error(&mut self, packet: &[u8]) {
        fn do_parse(inner: &mut ConnectionInner, packet: &[u8]) -> Result<(), ParseError> {
            let err = X11Error::try_parse(packet, &inner.ext_info)?;
            inner.server_sent += 1;
            inner.record_message(
                MessageMetadata {
                    kind: MessageKind::ServerMessage,
                    seq: err.sequence,
                    event_code: 0,
                },
                packet,
            );
            inner.record_dbg(
                &format!("{err:?}"),
                MessageKind::ServerMessage,
                err.sequence,
            );
            // Remove a pending request if it failed
            let next_pending = inner.pending_replies.front().map(|r| r.seqno);
            if next_pending == Some(err.sequence) {
                let _ = inner.pending_replies.pop_front();
            }

            Ok(())
        }
        if let Err(e) = do_parse(self, packet) {
            eprintln!("Error while parsing an X11 error: {:?}", e);
        }
    }

    /// Handle an X11 event sent by the server
    pub fn server_event(&mut self, packet: &[u8]) {
        fn do_parse(inner: &mut ConnectionInner, packet: &[u8]) -> Result<(), ParseError> {
            let event = Event::parse(packet, &inner.ext_info)?;
            inner.server_sent += 1;
            inner.record_message(
                MessageMetadata {
                    kind: MessageKind::ServerMessage,
                    seq: event.wire_sequence_number().unwrap_or(0),
                    event_code: event.response_type(),
                },
                packet,
            );
            inner.record_dbg(
                &format!("{event:?}"),
                MessageKind::ServerMessage,
                event.wire_sequence_number().unwrap_or(0),
            );
            Ok(())
        }

        if let Err(e) = do_parse(self, packet) {
            eprintln!("Error while parsing an X11 event: {:?}", e);
        }
    }

    /// Handle a reply sent by the server
    pub fn server_reply(&mut self, packet: &[u8]) {
        fn do_parse(inner: &mut ConnectionInner, packet: &[u8]) -> Result<(), ParseError> {
            // Figure out information about the request that is being answered.
            let request = match inner.pending_replies.pop_front() {
                None => {
                    println!("server: Got unexpected reply {:?}", packet);
                    return Ok(());
                }
                Some(request) => request,
            };

            // Sanity check: The sequence number must match the expected one.
            let seqno = u16::from_ne_bytes(packet[2..4].try_into().unwrap());
            if request.seqno != seqno {
                println!(
                    "Expected reply with seqno={}, but got seqno={}",
                    request.seqno, seqno,
                );
            }
            // Actually parse the reply
            let (reply, _remaining) = (request.parser)(packet, &mut Vec::new())?;
            inner.server_sent += 1;
            inner.record_message(
                MessageMetadata {
                    kind: MessageKind::ServerMessage,
                    seq: seqno,
                    event_code: 0,
                },
                packet,
            );
            inner.record_dbg(&format!("{reply:?}"), MessageKind::ServerMessage, seqno);

            // If it is a reply to a QueryExtension request and the extension is present, update
            // our state (add the extension to our ext_info).
            if let Some(extension) = request.queried_extension {
                if let Reply::QueryExtension(reply) = reply {
                    if reply.present {
                        let info = ExtensionInformation {
                            major_opcode: reply.major_opcode,
                            first_event: reply.first_event,
                            first_error: reply.first_error,
                        };
                        inner.ext_info.add_extension(extension, info);
                    }
                }
            } else if let Reply::ListFontsWithInfo(reply) = reply {
                // There is one request that can generate multiple replies: ListFontsWithInfo. Mark it
                // as pending again if it is not the last reply. This makes 'xlsfonts -l' work.
                if !reply.name.is_empty() {
                    inner.pending_replies.push_front(request);
                }
            }

            Ok(())
        }
        if let Err(e) = do_parse(self, packet) {
            eprintln!("Error while parsing an X11 event: {:?}", e);
        }
    }

    fn record_message(&mut self, metadata: MessageMetadata, payload: &[u8]) {
        self.recorder.record_message(metadata, payload);
    }

    fn record_dbg(&mut self, raw_info: &str, kind: MessageKind, seq: u16) {
        self.recorder
            .record_dbg(raw_info, kind, self.client_sent, self.server_sent, seq);
    }
}

#[derive(Debug)]
struct Recorder {
    binary_file: File,
    log_file: File,
}

impl Recorder {
    fn record_message(&mut self, metadata: MessageMetadata, payload: &[u8]) {
        let _ = self
            .binary_file
            .write(&serialize_message(metadata, payload))
            .unwrap();
    }

    fn record_dbg(
        &mut self,
        raw_info: &str,
        kind: MessageKind,
        client_sent: usize,
        server_sent: usize,
        seq: u16,
    ) {
        let line = format!(
            "{}({seq}), client_sent = {client_sent}, server_sent = {server_sent}\n{raw_info}\n",
            kind.format()
        );
        let _ = self.log_file.write(line.as_bytes()).unwrap();
    }
}

impl Default for Recorder {
    fn default() -> Self {
        let mut binary_file = None;
        for i in 0..999 {
            let check = format!("performance-test/event-source/ser_events{i}.log");
            if std::fs::metadata(&check).is_err() {
                binary_file = Some(
                    std::fs::OpenOptions::new()
                        .create(true)
                        .write(true)
                        .open(check)
                        .unwrap(),
                );
                break;
            }
        }
        let mut log_file = None;
        for i in 0..999 {
            let check = format!("performance-test/event-source/dbg_events{i}.log");
            if std::fs::metadata(&check).is_err() {
                log_file = Some(
                    std::fs::OpenOptions::new()
                        .create(true)
                        .write(true)
                        .open(check)
                        .unwrap(),
                );
                break;
            }
        }
        Self {
            binary_file: binary_file.unwrap(),
            log_file: log_file.unwrap(),
        }
    }
}

/// Representation of a request that was not yet answered.
struct PendingReply {
    /// The sequence number of the request.
    seqno: u16,

    /// A function that can parse the reply.
    parser: ReplyParsingFunction,

    /// For QueryExtension requests: The extension that was queried
    queried_extension: Option<String>,
}

impl std::fmt::Debug for PendingReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingReply")
            .field("seqno", &self.seqno)
            .field("parser", &"<???>")
            .field("queried_extension", &self.queried_extension)
            .finish()
    }
}

/// Information about known extensions.
#[derive(Debug, Default)]
struct ExtInfo {
    /// A list of extension names and their information
    exts: Vec<(String, ExtensionInformation)>,
}

impl ExtInfo {
    /// Add a new extension to the state
    fn add_extension(&mut self, name: String, info: ExtensionInformation) {
        self.exts.push((name, info))
    }
}

impl ExtInfoProvider for ExtInfo {
    fn get_from_major_opcode(&self, major_opcode: u8) -> Option<(&str, ExtensionInformation)> {
        self.exts
            .iter()
            .find(|(_, ext)| ext.major_opcode == major_opcode)
            .map(|(s, ext)| (s.as_ref(), *ext))
    }

    fn get_from_event_code(&self, event_code: u8) -> Option<(&str, ExtensionInformation)> {
        self.exts
            .iter()
            .filter(|(_, ext)| ext.first_event <= event_code)
            .max_by_key(|(_, ext)| ext.first_event)
            .map(|(s, ext)| (s.as_ref(), *ext))
    }

    fn get_from_error_code(&self, error_code: u8) -> Option<(&str, ExtensionInformation)> {
        self.exts
            .iter()
            .filter(|(_, ext)| ext.first_error <= error_code)
            .max_by_key(|(_, ext)| ext.first_event)
            .map(|(s, ext)| (s.as_ref(), *ext))
    }
}
