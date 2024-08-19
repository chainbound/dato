#![doc = include_str!("../README.md")]
#![warn(missing_debug_implementations, missing_docs, rustdoc::all)]
#![deny(unused_must_use, rust_2018_idioms)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

mod common;
pub use common::{
    CertifiedLog, CertifiedReadMessageResponse, CertifiedRecord, CertifiedUnavailableMessage, Log,
    Message, Namespace, ReadError, ReadMessageResponse, Record, Timestamp, UnavailableMessage,
    ValidatorIdentity, WriteError,
};

mod primitives;
pub use primitives::bls;

mod client;
pub use client::{Client, ClientSpec};

mod validator;
pub use validator::{Validator, ValidatorSpec};

mod registry;
pub use registry::{FilesystemRegistry, Registry, SmartContractRegistry};
