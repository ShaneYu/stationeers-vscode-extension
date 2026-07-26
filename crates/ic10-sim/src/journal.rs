use std::collections::{BTreeMap, VecDeque};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct SymbolId(pub u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum EffectActor {
    Ic {
        cpu: usize,
        source: SymbolId,
        line: usize,
    },
    Behaviour {
        device: usize,
        model: SymbolId,
        version: u32,
    },
    ScriptedDriver {
        driver: SymbolId,
        rule: u32,
    },
    Scenario,
    Debugger,
    Scheduler,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum EffectTarget {
    Register {
        cpu: usize,
        register: u8,
    },
    Stack {
        cpu: usize,
        address: u16,
    },
    CpuPc {
        cpu: usize,
    },
    CpuState {
        cpu: usize,
    },
    CpuError {
        cpu: usize,
    },
    CpuOperations {
        cpu: usize,
    },
    CpuRandom {
        cpu: usize,
    },
    SchedulerCpu,
    Tick,
    DeviceField {
        device: usize,
        field: SymbolId,
    },
    DeviceSlot {
        device: usize,
        slot: u16,
        field: SymbolId,
    },
    DeviceMemory {
        device: usize,
        address: u32,
    },
    NetworkChannel {
        network: usize,
        channel: u8,
    },
    BehaviourState {
        device: usize,
        key: SymbolId,
    },
    DriverState {
        driver: SymbolId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadEffect {
    pub actor: EffectActor,
    pub target: EffectTarget,
    pub value_bits: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteEffect {
    pub actor: EffectActor,
    pub target: EffectTarget,
    pub before_bits: u64,
    pub after_bits: u64,
    /// True even when the raw value did not change. This distinguishes an
    /// attempted store from the absence of a store.
    pub attempted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SequencedWriteEffect {
    pub sequence: u64,
    pub effect: WriteEffect,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectBatch {
    pub reads: Vec<ReadEffect>,
    pub writes: Vec<WriteEffect>,
}

#[derive(Clone, Debug, Default)]
pub struct EffectJournal {
    enabled: bool,
    symbols: Vec<String>,
    symbol_ids: BTreeMap<String, SymbolId>,
    pending: EffectBatch,
    next_write_sequence: u64,
    write_history: VecDeque<SequencedWriteEffect>,
}

impl EffectJournal {
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.pending = EffectBatch::default();
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn intern(&mut self, value: &str) -> SymbolId {
        if let Some(id) = self.symbol_ids.get(value) {
            return *id;
        }
        let id = SymbolId(self.symbols.len() as u32);
        self.symbols.push(value.to_owned());
        self.symbol_ids.insert(value.to_owned(), id);
        id
    }

    pub fn resolve(&self, id: SymbolId) -> Option<&str> {
        self.symbols.get(id.0 as usize).map(String::as_str)
    }

    pub fn read(&mut self, actor: EffectActor, target: EffectTarget, value: f64) {
        if self.enabled {
            self.pending.reads.push(ReadEffect {
                actor,
                target,
                value_bits: value.to_bits(),
            });
        }
    }

    pub fn write(&mut self, actor: EffectActor, target: EffectTarget, before: f64, after: f64) {
        self.write_bits(actor, target, before.to_bits(), after.to_bits());
    }

    pub fn write_bits(
        &mut self,
        actor: EffectActor,
        target: EffectTarget,
        before_bits: u64,
        after_bits: u64,
    ) {
        if self.enabled {
            let effect = WriteEffect {
                actor,
                target,
                before_bits,
                after_bits,
                attempted: true,
            };
            self.pending.writes.push(effect.clone());
            self.next_write_sequence = self.next_write_sequence.wrapping_add(1);
            self.write_history.push_back(SequencedWriteEffect {
                sequence: self.next_write_sequence,
                effect,
            });
        }
    }

    pub fn take(&mut self) -> EffectBatch {
        std::mem::take(&mut self.pending)
    }

    pub fn extend(&mut self, mut batch: EffectBatch) {
        if self.enabled {
            self.pending.reads.append(&mut batch.reads);
            for effect in batch.writes.drain(..) {
                self.next_write_sequence = self.next_write_sequence.wrapping_add(1);
                self.write_history.push_back(SequencedWriteEffect {
                    sequence: self.next_write_sequence,
                    effect: effect.clone(),
                });
                self.pending.writes.push(effect);
            }
        }
    }

    pub fn write_sequence(&self) -> u64 {
        self.next_write_sequence
    }

    pub fn writes_after(&self, sequence: u64) -> Vec<SequencedWriteEffect> {
        self.write_history
            .iter()
            .filter(|write| write.sequence > sequence)
            .cloned()
            .collect()
    }

    pub fn acknowledge_writes_through(&mut self, sequence: u64) {
        while self
            .write_history
            .front()
            .is_some_and(|write| write.sequence <= sequence)
        {
            self.write_history.pop_front();
        }
    }

    pub fn restore_write_sequence(&mut self, sequence: u64) {
        self.next_write_sequence = sequence;
        self.write_history.clear();
        self.pending = EffectBatch::default();
    }

    pub fn symbols(&self) -> &[String] {
        &self.symbols
    }
}
