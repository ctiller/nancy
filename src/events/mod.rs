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

pub mod index;
pub mod reader;
pub mod writer;

use serde::{Deserialize, Serialize};

use crate::schema::registry::EventPayload;

#[derive(Debug, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub id: String,
    pub did: String,
    pub payload: EventPayload,
    pub signature: String,
}

// DOCUMENTED_BY: [docs/adr/0006-events-library.md]
