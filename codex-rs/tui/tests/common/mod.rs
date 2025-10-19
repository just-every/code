//! Common test utilities for spec-kit
//!
//! FORK-SPECIFIC (just-every/code): Spec-kit test infrastructure (MAINT-3, Phase 3)

pub mod mock_mcp;
pub mod integration_harness;

pub use mock_mcp::MockMcpManager;
pub use integration_harness::{IntegrationTestContext, StateBuilder, EvidenceVerifier};
