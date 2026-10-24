//! Error types of the protocol crate.

use crate::request::ReadRequest;

/// A byte buffer could not be turned into a [`Report`](crate::Report).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    /// The buffer has a length that is neither a bare 63-byte payload nor a
    /// 64-byte report with a leading report ID.
    #[error("input report has {len} bytes, expected 63 (or 64 with the report ID)")]
    BadLength {
        /// Length of the rejected buffer.
        len: usize,
    },
    /// A 64-byte buffer did not start with report ID 1.
    #[error("input report has report ID {id:#04x}, expected 0x01")]
    WrongReportId {
        /// The report ID that was found.
        id: u8,
    },
    /// A hex string could not be parsed.
    #[error("invalid hex: {0}")]
    BadHex(String),
}

/// The reports collected for a request do not form a complete response.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResponseError {
    /// Some chunks never arrived.
    #[error("incomplete response to {request}: missing chunk(s) {missing:?} of {expected}")]
    Incomplete {
        /// The request that was being answered.
        request: ReadRequest,
        /// Chunk indices that are missing.
        missing: Vec<u16>,
        /// Number of chunks the response should have.
        expected: u16,
    },
    /// A report does not belong to the response of this request.
    #[error("report {report} does not answer {request}")]
    Unrelated {
        /// The request that was being answered.
        request: ReadRequest,
        /// Hex dump of the header of the offending report.
        report: String,
    },
}

/// A complete response could not be decoded into its typed model.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    /// The response belongs to a different request than the decoder expects.
    #[error("cannot decode the response to {got} as {expected}")]
    WrongRequest {
        /// Name of the model the caller asked for.
        expected: &'static str,
        /// The request whose response was supplied.
        got: ReadRequest,
    },
    /// The response carries fewer bytes than the layout needs.
    #[error("{what}: need {need} byte(s), got {got}")]
    TooShort {
        /// What was being decoded.
        what: &'static str,
        /// Minimum number of bytes needed.
        need: usize,
        /// Number of bytes available.
        got: usize,
    },
    /// A chunk of a table declares a different length than the spec says.
    #[error("{what}: chunk {chunk} declares {got} byte(s), expected {expected}")]
    ChunkLength {
        /// What was being decoded.
        what: &'static str,
        /// Chunk index.
        chunk: u16,
        /// Length the spec expects for this chunk.
        expected: usize,
        /// Length declared in byte 4 of the report.
        got: usize,
    },
}
