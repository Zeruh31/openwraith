//! Tests of the worker against the simulator (virtual time, synchronous
//! unless stated otherwise), the generated frontend files and the source
//! rules.

mod admission;
mod backup;
mod bindings;
mod commands_sync;
mod connection;
mod fault;
mod fixtures;
mod foreign;
mod gate;
mod generated;
mod harness;
mod interleavings;
mod light;
mod profile;
mod restore;
mod seeds;
mod source_rules;
mod trigger;
