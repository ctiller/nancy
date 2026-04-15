#![recursion_limit = "256"]

// Copyright 2026 Craig Tiller
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.


pub mod commands;
pub mod coordinator;
pub mod dreamer;
pub mod eval;
pub mod events;
pub mod git;
pub mod grind;
pub mod introspection;
pub mod llm;
pub mod personas;
pub mod pre_review;
pub mod schema;
pub mod tasks;
pub mod tools;

pub mod agent;
#[cfg(test)]
pub mod debug;
