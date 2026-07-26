use ic10_sim::{EffectActor, Scalar, Simulator};
use serde::{Deserialize, Serialize};

use crate::evaluator::{
    AssignableTarget, assignable_target_matches_write, set_value_as, validate_assignable_target,
};
use crate::schema::{ScriptAction, ScriptedDriver};

const MAX_CASCADES_PER_PUMP: usize = 64;
const MAX_ACTIONS_PER_CASE: usize = 10_000;
const MAX_PENDING_EVENTS: usize = 2_048;
const STATE_FORMAT_VERSION: u32 = 1;
const STATE_KEY: &str = "scenario.scripted/v1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct ScheduledAction {
    tick: u64,
    sequence: u64,
    driver: usize,
    rule: usize,
    action: ScriptAction,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
struct PersistedState {
    pending: Vec<ScheduledAction>,
    sequence: u64,
    actions_run: usize,
    write_cursor: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StateEnvelope {
    format_version: u32,
    definition_fingerprint: u64,
    state: PersistedState,
}

pub struct ScriptDrivers<'a> {
    definitions: &'a [ScriptedDriver],
    triggers: Vec<AssignableTarget>,
    definition_fingerprint: u64,
    state: PersistedState,
}

impl<'a> ScriptDrivers<'a> {
    pub fn new(
        simulator: &mut Simulator,
        thread: usize,
        definitions: &'a [ScriptedDriver],
    ) -> Result<Self, String> {
        // Write reactions consume the simulator's ordered effect stream.
        // Enabling journaling here is protocol-neutral and leaves consumers
        // free to take the ordinary effect batches independently.
        simulator.set_journaling(true);
        let definition_fingerprint = definition_fingerprint(definitions)?;
        let mut runtime = Self {
            definitions,
            triggers: Vec::new(),
            definition_fingerprint,
            state: PersistedState::default(),
        };
        runtime.validate_targets(simulator, thread)?;
        runtime.reload(simulator)?;
        if simulator.test_driver_state(STATE_KEY).is_none() {
            runtime.state.write_cursor = simulator.write_sequence();
            runtime.persist(simulator, EffectActor::Scenario)?;
        }
        Ok(runtime)
    }

    pub fn has_pending(&self) -> bool {
        !self.state.pending.is_empty()
    }

    /// Restore runtime-owned state after a simulator checkpoint restore.
    pub fn reload(&mut self, simulator: &Simulator) -> Result<(), String> {
        let Some(bytes) = simulator.test_driver_state(STATE_KEY) else {
            self.state = PersistedState::default();
            return Ok(());
        };
        let envelope: StateEnvelope = serde_json::from_slice(bytes)
            .map_err(|error| format!("scripted driver state is corrupt: {error}"))?;
        if envelope.format_version != STATE_FORMAT_VERSION {
            return Err(format!(
                "unsupported scripted driver state version {}",
                envelope.format_version
            ));
        }
        if envelope.definition_fingerprint != self.definition_fingerprint {
            return Err("scripted driver state belongs to different driver definitions".to_owned());
        }
        self.state = envelope.state;
        Ok(())
    }

    pub fn pump(&mut self, simulator: &mut Simulator, thread: usize) -> Result<(), String> {
        let due_count = self
            .state
            .pending
            .iter()
            .take_while(|a| a.tick <= simulator.tick)
            .count();
        let due: Vec<ScheduledAction> = self.state.pending.drain(..due_count).collect();
        for pending in due {
            self.execute(
                simulator,
                thread,
                pending.driver,
                pending.rule,
                &pending.action,
            )?;
        }
        self.react_to_writes(simulator, thread)?;
        self.persist(simulator, EffectActor::Scenario)
    }

    fn validate_targets(&mut self, simulator: &Simulator, thread: usize) -> Result<(), String> {
        self.triggers.clear();
        for (driver_index, driver) in self.definitions.iter().enumerate() {
            for (rule_index, rule) in driver.rules.iter().enumerate() {
                self.triggers.push(
                    validate_assignable_target(simulator, thread, &rule.when.target).map_err(
                        |message| {
                            provenance(
                                driver,
                                rule_index,
                                &format!("invalid trigger target: {message}"),
                            )
                        },
                    )?,
                );
                validate_actions(simulator, thread, driver, rule_index, &rule.actions)?;
            }
            if driver.id.trim().is_empty() {
                return Err(format!(
                    "scripted driver #{} has an empty id",
                    driver_index + 1
                ));
            }
        }
        Ok(())
    }

    fn react_to_writes(&mut self, simulator: &mut Simulator, thread: usize) -> Result<(), String> {
        for cascade in 0..=MAX_CASCADES_PER_PUMP {
            let writes = simulator.writes_after(self.state.write_cursor);
            if writes.is_empty() {
                return Ok(());
            }
            let mut triggered = Vec::new();
            for write in &writes {
                let mut flat = 0;
                for (driver_index, driver) in self.definitions.iter().enumerate() {
                    for (rule_index, rule) in driver.rules.iter().enumerate() {
                        let matches = assignable_target_matches_write(
                            simulator,
                            &self.triggers[flat],
                            &write.effect.target,
                        );
                        let equals = rule
                            .when
                            .equals
                            .as_ref()
                            .map(scalar)
                            .is_none_or(|expected| expected.to_bits() == write.effect.after_bits);
                        if matches && equals {
                            triggered.push((driver_index, rule_index, rule.actions.clone()));
                        }
                        flat += 1;
                    }
                }
            }
            self.state.write_cursor = writes
                .last()
                .map_or(self.state.write_cursor, |write| write.sequence);
            simulator.acknowledge_writes_through(self.state.write_cursor);
            if triggered.is_empty() {
                return Ok(());
            }
            if cascade == MAX_CASCADES_PER_PUMP {
                let (driver, rule, _) = &triggered[0];
                return Err(provenance(
                    &self.definitions[*driver],
                    *rule,
                    "reaction cascade exceeded limit 64 (possible scripted cycle)",
                ));
            }
            for (driver, rule, actions) in triggered {
                for action in &actions {
                    self.execute(simulator, thread, driver, rule, action)?;
                }
            }
        }
        Ok(())
    }

    fn execute(
        &mut self,
        simulator: &mut Simulator,
        thread: usize,
        driver_index: usize,
        rule_index: usize,
        action: &ScriptAction,
    ) -> Result<(), String> {
        self.state.actions_run += 1;
        let driver = &self.definitions[driver_index];
        if self.state.actions_run > MAX_ACTIONS_PER_CASE {
            return Err(provenance(
                driver,
                rule_index,
                "action limit 10000 exceeded",
            ));
        }
        let actor = simulator.scripted_driver_actor(&driver.id, rule_index);
        let result = match action {
            ScriptAction::Set { target, value } => {
                set_value_as(simulator, thread, target, scalar(value), actor)
            }
            ScriptAction::Publish {
                network,
                channel,
                value,
            } => {
                let target = format!("network(\"{network}\").Channel{channel}");
                set_value_as(simulator, thread, &target, scalar(value), actor)
            }
            ScriptAction::MoveSlot { from, to } => {
                let (from_device, from_slot) = slot_endpoint(simulator, from)?;
                let (to_device, to_slot) = slot_endpoint(simulator, to)?;
                simulator.move_slot_item_as(from_device, from_slot, to_device, to_slot, actor)
            }
            ScriptAction::Schedule {
                after_ticks,
                actions,
            } => {
                if self.state.pending.len() + actions.len() > MAX_PENDING_EVENTS {
                    return Err(provenance(
                        driver,
                        rule_index,
                        "pending event limit 2048 exceeded",
                    ));
                }
                for action in actions {
                    let tick = simulator.tick.saturating_add(*after_ticks);
                    let sequence = self.state.sequence;
                    self.state.sequence = self.state.sequence.wrapping_add(1);
                    self.state.pending.push(ScheduledAction {
                        tick,
                        sequence,
                        driver: driver_index,
                        rule: rule_index,
                        action: action.clone(),
                    });
                }
                self.state.pending.sort_by_key(|a| (a.tick, a.sequence));
                Ok(())
            }
        };
        result.map_err(|message| provenance(driver, rule_index, &message))
    }

    fn persist(&self, simulator: &mut Simulator, actor: EffectActor) -> Result<(), String> {
        let state = serde_json::to_vec(&StateEnvelope {
            format_version: STATE_FORMAT_VERSION,
            definition_fingerprint: self.definition_fingerprint,
            state: self.state.clone(),
        })
        .map_err(|error| format!("could not serialize scripted driver state: {error}"))?;
        simulator.set_test_driver_state(STATE_KEY, state, actor);
        Ok(())
    }
}

fn definition_fingerprint(definitions: &[ScriptedDriver]) -> Result<u64, String> {
    let bytes = serde_json::to_vec(definitions)
        .map_err(|error| format!("could not fingerprint scripted drivers: {error}"))?;
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in (bytes.len() as u64).to_le_bytes().into_iter().chain(bytes) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    Ok(hash)
}

fn validate_actions(
    simulator: &Simulator,
    thread: usize,
    driver: &ScriptedDriver,
    rule: usize,
    actions: &[ScriptAction],
) -> Result<(), String> {
    for action in actions {
        match action {
            ScriptAction::Set { target, .. } => {
                validate_assignable_target(simulator, thread, target).map_err(|message| {
                    provenance(driver, rule, &format!("invalid set target: {message}"))
                })?;
            }
            ScriptAction::MoveSlot { from, to } => {
                slot_endpoint(simulator, from)
                    .map_err(|message| provenance(driver, rule, &message))?;
                slot_endpoint(simulator, to)
                    .map_err(|message| provenance(driver, rule, &message))?;
            }
            ScriptAction::Publish {
                network, channel, ..
            } => {
                if *channel > 7 || simulator.world.network_index(network).is_none() {
                    return Err(provenance(
                        driver,
                        rule,
                        "publish references an invalid network/channel",
                    ));
                }
            }
            ScriptAction::Schedule { actions, .. } => {
                validate_actions(simulator, thread, driver, rule, actions)?;
            }
        }
    }
    Ok(())
}

fn slot_endpoint(simulator: &Simulator, text: &str) -> Result<(usize, usize), String> {
    let rest = text
        .trim()
        .strip_prefix("device(\"")
        .ok_or_else(|| format!("invalid slot endpoint `{text}`"))?;
    let (device, slot) = rest
        .split_once("\").slot[")
        .ok_or_else(|| format!("invalid slot endpoint `{text}`"))?;
    let slot = slot
        .strip_suffix(']')
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or_else(|| format!("invalid slot endpoint `{text}`"))?;
    let device = simulator
        .world
        .device_index(device)
        .ok_or_else(|| format!("unknown device in slot endpoint `{text}`"))?;
    if !simulator.world.devices[device].slots.contains_key(&slot) {
        return Err(format!("slot endpoint `{text}` is unavailable"));
    }
    Ok((device, slot))
}

fn scalar(value: &Scalar) -> f64 {
    value.as_f64().unwrap_or(f64::NAN)
}

fn provenance(driver: &ScriptedDriver, rule: usize, message: &str) -> String {
    let rule_name = driver.rules[rule]
        .name
        .as_deref()
        .map_or_else(|| format!("#{}", rule + 1), str::to_owned);
    format!(
        "scripted driver `{}` model {}@{} rule `{}` failed: {}",
        driver.id, driver.model, driver.version, rule_name, message
    )
}
