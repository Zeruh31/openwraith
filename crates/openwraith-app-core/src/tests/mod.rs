//! Tests of the worker against the simulator (virtual time, synchronous
//! unless stated otherwise), the generated frontend files and the source
//! rules.

mod admission;
mod auto;
mod backup;
mod bindings;
mod coexist;
mod commands_sync;
mod connection;
mod editors;
mod fault;
mod fixtures;
mod foreign;
mod gate;
mod generated;
mod harness;
mod hygiene;
mod interleavings;
mod keymap;
mod light;
mod profile;
mod recovery;
mod recovery_holds;
mod restore;
mod seeds;
mod share;
mod source_rules;
mod travel;
mod trigger;
