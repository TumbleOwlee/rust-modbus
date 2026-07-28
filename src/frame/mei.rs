//! Encapsulated Interface Transport bodies (FR-R-070 … FR-R-077).
//!
//! Function code 43 is a container: a 1-byte MEI type selects a body the crate
//! either knows (Read Device Identification) or carries whole (CANopen, and
//! anything unnamed).

use winnow::Parser;
use winnow::binary::be_u8;
use winnow::token::{rest, take};

use crate::error::{Error, Result};
use crate::parse::{self, Input, ParseResult};

/// MEI type 13 — CANopen General Reference (FR-R-071).
pub(super) const MEI_CANOPEN: u8 = 13;

/// MEI type 14 — Read Device Identification (FR-R-071).
pub(super) const MEI_READ_DEVICE_ID: u8 = 14;

/// Which conformity class of device identification to read (FR-R-074).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadDeviceIdCode {
    /// 1 — the basic identification objects, streamed.
    Basic,
    /// 2 — the regular identification objects, streamed.
    Regular,
    /// 3 — the extended identification objects, streamed.
    Extended,
    /// 4 — one specific identification object.
    Individual,
}

/// One device identification object (FR-R-075).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceIdObject {
    /// Object identifier.
    pub id: u8,
    /// Object value; its length is carried on the wire, not fixed.
    pub value: Vec<u8>,
}

/// The body of an Encapsulated Interface Transport request (FR-R-070).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeiRequest {
    /// MEI 13 — CANopen General Reference, carried whole (FR-R-072).
    CanOpen {
        /// Every remaining PDU byte.
        data: Vec<u8>,
    },
    /// MEI 14 — Read Device Identification (FR-R-073).
    ReadDeviceIdentification {
        /// Which conformity class to read (FR-R-074).
        read_device_id_code: ReadDeviceIdCode,
        /// First object to return.
        object_id: u8,
    },
    /// Any MEI type the crate does not name, carried whole (FR-R-071).
    Other {
        /// The raw MEI type byte.
        mei_type: u8,
        /// Every remaining PDU byte.
        data: Vec<u8>,
    },
}

/// The body of an Encapsulated Interface Transport response (FR-R-070).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeiResponse {
    /// MEI 13 — CANopen General Reference, carried whole (FR-R-072).
    CanOpen {
        /// Every remaining PDU byte.
        data: Vec<u8>,
    },
    /// MEI 14 — Read Device Identification (FR-R-075).
    ReadDeviceIdentification {
        /// The conformity class read.
        read_device_id_code: ReadDeviceIdCode,
        /// Conformity level the device reports.
        conformity_level: u8,
        /// Whether a further request is needed to read the rest (FR-R-076).
        more_follows: bool,
        /// Object id to ask for next when `more_follows` is set.
        next_object_id: u8,
        /// The objects returned (FR-R-077).
        objects: Vec<DeviceIdObject>,
    },
    /// Any MEI type the crate does not name, carried whole (FR-R-071).
    Other {
        /// The raw MEI type byte.
        mei_type: u8,
        /// Every remaining PDU byte.
        data: Vec<u8>,
    },
}

/// Wire value of the more-follows indicator meaning "no more" (FR-R-076).
const MORE_FOLLOWS_NO: u8 = 0x00;

/// Wire value of the more-follows indicator meaning "partial" (FR-R-076).
const MORE_FOLLOWS_YES: u8 = 0xFF;

impl ReadDeviceIdCode {
    /// Decode a read device id code (FR-R-074).
    pub(super) fn decode(raw: u8) -> Result<Self> {
        match raw {
            1 => Ok(Self::Basic),
            2 => Ok(Self::Regular),
            3 => Ok(Self::Extended),
            4 => Ok(Self::Individual),
            other => Err(Error::OutOfRange {
                field: "read device id code",
                value: u32::from(other),
                min: 1,
                max: 4,
            }),
        }
    }

    /// The code's wire byte.
    pub(super) fn encode(self) -> u8 {
        match self {
            Self::Basic => 1,
            Self::Regular => 2,
            Self::Extended => 3,
            Self::Individual => 4,
        }
    }
}

/// Decode an Encapsulated Interface Transport request body (FR-R-070).
pub(super) fn decode_request(input: &mut Input<'_>) -> ParseResult<MeiRequest> {
    let mei_type = be_u8.parse_next(input)?;
    Ok(match mei_type {
        MEI_CANOPEN => MeiRequest::CanOpen {
            data: opaque(input)?,
        },
        MEI_READ_DEVICE_ID => {
            let raw = be_u8.parse_next(input)?;
            MeiRequest::ReadDeviceIdentification {
                read_device_id_code: parse::lift(ReadDeviceIdCode::decode(raw))?,
                object_id: be_u8.parse_next(input)?,
            }
        }
        other => MeiRequest::Other {
            mei_type: other,
            data: opaque(input)?,
        },
    })
}

/// Encode an Encapsulated Interface Transport request body (FR-R-070).
pub(super) fn encode_request(request: &MeiRequest) -> Vec<u8> {
    match *request {
        MeiRequest::CanOpen { ref data } => opaque_body(MEI_CANOPEN, data),
        MeiRequest::ReadDeviceIdentification {
            read_device_id_code,
            object_id,
        } => vec![MEI_READ_DEVICE_ID, read_device_id_code.encode(), object_id],
        MeiRequest::Other { mei_type, ref data } => opaque_body(mei_type, data),
    }
}

/// Decode an Encapsulated Interface Transport response body (FR-R-070).
pub(super) fn decode_response(input: &mut Input<'_>) -> ParseResult<MeiResponse> {
    let mei_type = be_u8.parse_next(input)?;
    Ok(match mei_type {
        MEI_CANOPEN => MeiResponse::CanOpen {
            data: opaque(input)?,
        },
        MEI_READ_DEVICE_ID => decode_device_identification(input)?,
        other => MeiResponse::Other {
            mei_type: other,
            data: opaque(input)?,
        },
    })
}

/// Encode an Encapsulated Interface Transport response body (FR-R-070).
pub(super) fn encode_response(response: &MeiResponse) -> Result<Vec<u8>> {
    match *response {
        MeiResponse::CanOpen { ref data } => Ok(opaque_body(MEI_CANOPEN, data)),
        MeiResponse::ReadDeviceIdentification {
            read_device_id_code,
            conformity_level,
            more_follows,
            next_object_id,
            ref objects,
        } => {
            let count = u8::try_from(objects.len()).map_err(|_| Error::OutOfRange {
                field: "object count",
                value: u32::try_from(objects.len()).unwrap_or(u32::MAX),
                min: 0,
                max: u32::from(u8::MAX),
            })?;
            let mut bytes = vec![
                MEI_READ_DEVICE_ID,
                read_device_id_code.encode(),
                conformity_level,
                if more_follows {
                    MORE_FOLLOWS_YES
                } else {
                    MORE_FOLLOWS_NO
                },
                next_object_id,
                count,
            ];
            for object in objects {
                let length = u8::try_from(object.value.len()).map_err(|_| Error::OutOfRange {
                    field: "object length",
                    value: u32::try_from(object.value.len()).unwrap_or(u32::MAX),
                    min: 0,
                    max: u32::from(u8::MAX),
                })?;
                bytes.push(object.id);
                bytes.push(length);
                bytes.extend_from_slice(&object.value);
            }
            Ok(bytes)
        }
        MeiResponse::Other { mei_type, ref data } => Ok(opaque_body(mei_type, data)),
    }
}

/// Decode a Read Device Identification response body (FR-R-075).
fn decode_device_identification(input: &mut Input<'_>) -> ParseResult<MeiResponse> {
    let raw = be_u8.parse_next(input)?;
    let read_device_id_code = parse::lift(ReadDeviceIdCode::decode(raw))?;
    let conformity_level = be_u8.parse_next(input)?;
    let more_follows = match be_u8.parse_next(input)? {
        MORE_FOLLOWS_NO => false,
        MORE_FOLLOWS_YES => true,
        other => {
            return parse::fail(Error::IllegalValue {
                field: "more follows",
                value: u16::from(other),
            });
        }
    };
    let next_object_id = be_u8.parse_next(input)?;
    let object_count = usize::from(be_u8.parse_next(input)?);
    let body = rest.parse_next(input)?;
    let objects = parse::lift(parse::run_all(body, parse_object))?;
    if objects.len() != object_count {
        return parse::fail(Error::ByteCountMismatch {
            expected: object_count,
            actual: objects.len(),
        });
    }
    Ok(MeiResponse::ReadDeviceIdentification {
        read_device_id_code,
        conformity_level,
        more_follows,
        next_object_id,
        objects,
    })
}

/// Decode one device identification object (FR-R-075).
fn parse_object(input: &mut Input<'_>) -> ParseResult<DeviceIdObject> {
    let id = be_u8.parse_next(input)?;
    let length = usize::from(be_u8.parse_next(input)?);
    let value = take(length).parse_next(input)?;
    Ok(DeviceIdObject {
        id,
        value: value.to_vec(),
    })
}

/// Take every remaining byte as an opaque body (FR-R-071, FR-R-072).
fn opaque(input: &mut Input<'_>) -> ParseResult<Vec<u8>> {
    Ok(rest.parse_next(input)?.to_vec())
}

/// An MEI type followed by its opaque body.
fn opaque_body(mei_type: u8, data: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(data.len().saturating_add(1));
    bytes.push(mei_type);
    bytes.extend_from_slice(data);
    bytes
}
