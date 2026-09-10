//! z-tenant-onboarding — employee onboarding agent contract for T3N.
//!
//! # What it does
//!
//! Enrols one employee into the third-party HR systems a company uses, without
//! this contract — or the agent driving it, or any log along the way — ever
//! holding the employee's personal data.
//!
//! The enrolment body is built with `{{profile.<field>}}` markers. The host's
//! `http-with-placeholders` interface resolves them from the calling employee's
//! profile inside the enclave, immediately before the outbound request. WASM
//! memory only ever contains the marker text.
//!
//! # Why the vendor list is data, not code
//!
//! Every company onboards into a different set of vendors, and that set changes
//! more often than anyone wants to redeploy a TEE contract. So vendors live in
//! the tenant KV map `z:<tid>:onboarding-providers`, one JSON document each.
//!
//! Adding a vendor is a map write. No Rust change, no `cargo build`, no
//! re-registration, no new contract version to re-authorise in delegation
//! grants. The contract is the *mechanism*; the vendors are *configuration*.
//!
//! # Capabilities
//!
//! Determined entirely by the imports in `wit/world.wit` — there is no manifest.
//! (The reference implementation's doc-comment says otherwise; see
//! findings/BUGS.md BUG-07.)

#![warn(clippy::style, missing_debug_implementations)]
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

extern crate alloc;

#[cfg(target_arch = "wasm32")]
use alloc::string::String;
#[cfg(target_arch = "wasm32")]
use alloc::vec::Vec;

pub const CONTRACT_VERSION: &str = "0.1.1";

wit_bindgen::generate!({
    world: "tenant-onboarding",
    path: "wit",
    additional_derives: [
        serde::Deserialize,
        serde::Serialize,
    ],
    generate_all,
});

pub mod enroll;
pub mod registry;

struct Component;

#[cfg(target_arch = "wasm32")]
impl exports::z::tenant_onboarding::contracts::Guest for Component {
    fn list_providers(
        _req: exports::z::tenant_onboarding::contracts::GenericInput,
    ) -> Result<Vec<u8>, String> {
        // Takes no arguments — an absent `input` is as valid as `{}`.
        registry::list_providers()
    }

    fn enroll(
        req: exports::z::tenant_onboarding::contracts::GenericInput,
    ) -> Result<Vec<u8>, String> {
        let input = req.input.ok_or("enroll: missing input")?;
        enroll::enroll(&input)
    }
}

#[cfg(target_arch = "wasm32")]
export!(Component);
