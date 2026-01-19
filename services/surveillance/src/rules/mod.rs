//! Rules → Logic → Constraints → Arb Detector pipeline
//!
//! This module implements a pipeline for:
//! 1. Ingesting market rules/spec text
//! 2. Normalizing rules into canonical propositions
//! 3. Deriving logical constraints across markets
//! 4. Detecting arbitrage violations
//! 5. Managing human review queues

pub mod proposition;
pub mod ingest;
pub mod extract;
pub mod normalize;
pub mod confidence;
pub mod constraints;
pub mod arb_detector;
pub mod review_queue;
pub mod outputs;

pub use proposition::*;
pub use ingest::*;
pub use extract::*;
pub use normalize::*;
pub use confidence::*;
pub use constraints::*;
pub use arb_detector::*;
pub use review_queue::*;
pub use outputs::*;
