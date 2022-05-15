use bstr::ByteSlice;

const MESSAGE_START_TOKEN: &[u8; 8] = b"____MSGS";
const MESSAGE_END_TOKEN: &[u8; 8] = b"____MSGE";
const METADATA_LEN: usize = 8 + 2 + 1;

pub fn serialize_message(metadata: MessageMetadata, payload: &[u8]) -> Vec<u8> {
    let mut start = MESSAGE_START_TOKEN.to_vec();
    start.extend_from_slice(metadata.kind.serialize());
    start.extend_from_slice(&metadata.seq.to_le_bytes());
    start.push(metadata.event_code);
    start.extend_from_slice(payload);
    start.extend_from_slice(MESSAGE_END_TOKEN);
    start
}

pub fn parse_messages(raw: &[u8]) -> Vec<ReconstructedMessage> {
    assert_eq!(&raw[0..MESSAGE_START_TOKEN.len()], MESSAGE_START_TOKEN);

    let mut messages = vec![];
    let mut cursor = 0;
    while cursor < raw.len() {
        let (parsed, parsed_len) = parse_next(&raw[cursor..]);
        messages.push(parsed);
        cursor += parsed_len;
    }
    messages
}

fn parse_next(raw: &[u8]) -> (ReconstructedMessage, usize) {
    assert_eq!(&raw[0..MESSAGE_START_TOKEN.len()], MESSAGE_START_TOKEN);
    let msg_end = raw
        .find(&MESSAGE_END_TOKEN)
        .expect("Missing message end token");
    let payload = raw[MESSAGE_START_TOKEN.len() + METADATA_LEN..msg_end].to_vec();
    let len = MESSAGE_START_TOKEN.len() + METADATA_LEN + payload.len() + MESSAGE_END_TOKEN.len();
    (
        ReconstructedMessage {
            metadata: MessageMetadata::deserialize(
                &raw[MESSAGE_START_TOKEN.len()..MESSAGE_START_TOKEN.len() + METADATA_LEN],
            ),
            payload,
        },
        len,
    )
}

#[derive(Debug, Clone)]
pub struct ReconstructedMessage {
    pub metadata: MessageMetadata,
    pub payload: Vec<u8>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct MessageMetadata {
    pub kind: MessageKind,
    // Sequence number, may be omitted if irrelevant
    pub seq: u16,
    // Event code, may be omitted if irrelevant
    pub event_code: u8,
}

impl MessageMetadata {
    fn deserialize(metadata_slice: &[u8]) -> Self {
        let kind = MessageKind::deserialize(&metadata_slice[..8])
            .expect("Couldn't parse metata, no delimiter found");
        let seq = u16::from_le_bytes(metadata_slice[8..10].try_into().unwrap());
        let event_code = metadata_slice[10];
        MessageMetadata {
            kind,
            seq,
            event_code,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum MessageKind {
    // First event sent by a client to get information from the xorg-server, only sent once
    ClientSetup,
    // Response from the server with setup information, only sent once
    ServerSetup,
    // A message from the client to the server
    ClientMessage,
    // A message from the server to the client, an event, a reply, or an error
    ServerMessage,
}

const CLIENT_SETUP_TOKEN: &[u8] = b"CL____SE";
const SERVER_SETUP_TOKEN: &[u8] = b"SE____SE";
const CLIENT_MESSAGE_TOKEN: &[u8] = b"CL____ME";
const SERVER_MESSAGE_TOKEN: &[u8] = b"SE____ME";

impl MessageKind {
    fn serialize(&self) -> &'static [u8] {
        match self {
            MessageKind::ClientSetup => CLIENT_SETUP_TOKEN,
            MessageKind::ServerSetup => SERVER_SETUP_TOKEN,
            MessageKind::ClientMessage => CLIENT_MESSAGE_TOKEN,
            MessageKind::ServerMessage => SERVER_MESSAGE_TOKEN,
        }
    }

    pub fn format(&self) -> &'static str {
        match self {
            MessageKind::ClientSetup => "CLIENT_SETUP",
            MessageKind::ServerSetup => "SERVER_SETUP",
            MessageKind::ClientMessage => "CLIENT_MESSAGE",
            MessageKind::ServerMessage => "SERVER_MESSAGE",
        }
    }

    fn deserialize(kind_slice: &[u8]) -> Option<Self> {
        match kind_slice {
            CLIENT_SETUP_TOKEN => Some(MessageKind::ClientSetup),
            CLIENT_MESSAGE_TOKEN => Some(MessageKind::ClientMessage),
            SERVER_SETUP_TOKEN => Some(MessageKind::ServerSetup),
            SERVER_MESSAGE_TOKEN => Some(MessageKind::ServerMessage),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{parse_messages, serialize_message, MessageKind, MessageMetadata};

    #[test]
    fn can_serde_payloads() {
        let payload_1 = &[0, 1, 2, 3, 4];
        let meta_1 = MessageMetadata {
            kind: MessageKind::ClientSetup,
            seq: 0,
            event_code: 0,
        };
        let payload_2 = &[4, 3, 2, 1, 0];
        let meta_2 = MessageMetadata {
            kind: MessageKind::ServerSetup,
            seq: 0,
            event_code: 0,
        };
        let payload_3 = &[1, 1, 0, 1, 1];
        let meta_3 = MessageMetadata {
            kind: MessageKind::ClientMessage,
            seq: 1,
            event_code: 8,
        };
        let mut serialized = serialize_message(meta_1, payload_1);
        serialized.extend_from_slice(&serialize_message(meta_2, payload_2));
        serialized.extend_from_slice(&serialize_message(meta_3, payload_3));
        let deserialized = parse_messages(&serialized);
        assert_eq!(3, deserialized.len());
        assert_eq!(meta_1, deserialized[0].metadata);
        assert_eq!(payload_1, deserialized[0].payload.as_slice());
        assert_eq!(meta_2, deserialized[1].metadata);
        assert_eq!(payload_2, deserialized[1].payload.as_slice());
        assert_eq!(meta_3, deserialized[2].metadata);
        assert_eq!(payload_3, deserialized[2].payload.as_slice());
    }
}
