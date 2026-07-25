use ic10_sim::{Simulator, channel_index, direct_register_index};

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Number(f64),
    Boolean(bool),
    Text(String),
}

impl Value {
    pub fn truthy(&self) -> Result<bool, String> {
        match self {
            Self::Boolean(value) => Ok(*value),
            Self::Number(value) => Ok(*value != 0.0 && !value.is_nan()),
            Self::Text(_) => Err("text is not a boolean value".to_owned()),
        }
    }

    pub fn number(&self) -> Result<f64, String> {
        match self {
            Self::Number(value) => Ok(*value),
            Self::Boolean(value) => Ok(u8::from(*value) as f64),
            Self::Text(_) => Err("text is not a numeric value".to_owned()),
        }
    }

    pub fn display(&self) -> String {
        match self {
            Self::Number(value) => format_number(*value),
            Self::Boolean(value) => value.to_string(),
            Self::Text(value) => value.clone(),
        }
    }
}

pub fn evaluate(simulator: &Simulator, thread: usize, expression: &str) -> Result<Value, String> {
    evaluate_with_changed(simulator, thread, expression, &|_, _| false)
}

/// Evaluate an expression with stop-to-stop change information supplied by a
/// debugger. Scenario tests use [`evaluate`], while the DAP uses this entry
/// point for `changed(expression)` without maintaining a second expression
/// parser.
pub fn evaluate_with_changed(
    simulator: &Simulator,
    thread: usize,
    expression: &str,
    changed: &dyn Fn(&str, f64) -> bool,
) -> Result<Value, String> {
    let expression = expression.trim();
    if expression.is_empty() {
        return Err("expression is empty".to_owned());
    }
    validate_delimiters(expression)?;
    evaluate_inner(simulator, thread, trim_outer(expression), changed)
}

fn evaluate_inner(
    simulator: &Simulator,
    thread: usize,
    expression: &str,
    changed: &dyn Fn(&str, f64) -> bool,
) -> Result<Value, String> {
    for operators in [
        &["||"][..],
        &["&&"][..],
        &["==", "!=", "<=", ">=", "<", ">"][..],
        &["+", "-"][..],
        &["*", "/", "%"][..],
    ] {
        if let Some((left, operator, right)) = split_operator(expression, operators) {
            if left.trim().is_empty() || right.trim().is_empty() {
                return Err(format!("operator `{operator}` requires two operands"));
            }
            let left = evaluate_inner(simulator, thread, trim_outer(left.trim()), changed)?;
            if operator == "&&" && !left.truthy()? {
                return Ok(Value::Boolean(false));
            }
            if operator == "||" && left.truthy()? {
                return Ok(Value::Boolean(true));
            }
            let right = evaluate_inner(simulator, thread, trim_outer(right.trim()), changed)?;
            return binary(left, operator, right);
        }
    }
    if let Some(rest) = expression.strip_prefix('!') {
        return Ok(Value::Boolean(
            !evaluate_inner(simulator, thread, rest, changed)?.truthy()?,
        ));
    }
    if let Some(rest) = expression.strip_prefix('+') {
        return Ok(Value::Number(
            evaluate_inner(simulator, thread, rest, changed)?.number()?,
        ));
    }
    if let Some(rest) = expression.strip_prefix('-') {
        return Ok(Value::Number(
            -evaluate_inner(simulator, thread, rest, changed)?.number()?,
        ));
    }
    if let Some((function, argument)) = function_call(expression) {
        let value = evaluate_inner(simulator, thread, trim_outer(argument.trim()), changed)?;
        return match function {
            "abs" => Ok(Value::Number(value.number()?.abs())),
            "isnan" => Ok(Value::Boolean(value.number()?.is_nan())),
            "isfinite" => Ok(Value::Boolean(value.number()?.is_finite())),
            "changed" => {
                let number = value.number()?;
                Ok(Value::Boolean(changed(argument.trim(), number)))
            }
            _ => atom(simulator, thread, expression),
        };
    }
    atom(simulator, thread, expression)
}

fn binary(left: Value, operator: &str, right: Value) -> Result<Value, String> {
    match operator {
        "&&" => Ok(Value::Boolean(left.truthy()? && right.truthy()?)),
        "||" => Ok(Value::Boolean(left.truthy()? || right.truthy()?)),
        "==" | "!=" => {
            let equal = match (&left, &right) {
                (Value::Text(a), Value::Text(b)) => a == b,
                (Value::Boolean(a), Value::Boolean(b)) => a == b,
                _ => numeric_equal(left.number()?, right.number()?),
            };
            Ok(Value::Boolean(if operator == "==" {
                equal
            } else {
                !equal
            }))
        }
        "<" => Ok(Value::Boolean(left.number()? < right.number()?)),
        "<=" => Ok(Value::Boolean(left.number()? <= right.number()?)),
        ">" => Ok(Value::Boolean(left.number()? > right.number()?)),
        ">=" => Ok(Value::Boolean(left.number()? >= right.number()?)),
        "+" => Ok(Value::Number(left.number()? + right.number()?)),
        "-" => Ok(Value::Number(left.number()? - right.number()?)),
        "*" => Ok(Value::Number(left.number()? * right.number()?)),
        "/" => Ok(Value::Number(left.number()? / right.number()?)),
        "%" => Ok(Value::Number(left.number()? % right.number()?)),
        _ => Err(format!("unsupported operator `{operator}`")),
    }
}

fn numeric_equal(left: f64, right: f64) -> bool {
    (left.is_nan() && right.is_nan()) || left.to_bits() == right.to_bits()
}

fn atom(simulator: &Simulator, thread: usize, expression: &str) -> Result<Value, String> {
    let cpu = simulator
        .cpus
        .get(thread)
        .ok_or_else(|| format!("unknown thread {}", thread + 1))?;
    match expression {
        "true" => return Ok(Value::Boolean(true)),
        "false" => return Ok(Value::Boolean(false)),
        "tick" => return Ok(Value::Number(simulator.tick as f64)),
        "line" => {
            return Ok(Value::Number(cpu.current_line().unwrap_or(cpu.pc) as f64));
        }
        "operationCount" | "operationsThisTick" => {
            return Ok(Value::Number(cpu.operations_this_tick as f64));
        }
        "state" | "runState" => return Ok(Value::Text(format!("{:?}", cpu.state))),
        _ => {}
    }
    let resolved = cpu.program.resolve_alias(expression);
    if let Some(index) = direct_register_index(resolved) {
        return Ok(Value::Number(cpu.registers[index]));
    }
    if let Some(address) = indexed(expression, "stack[", "]") {
        return cpu
            .stack
            .get(address)
            .copied()
            .map(Value::Number)
            .ok_or_else(|| format!("stack address {address} is out of range"));
    }
    if let Some((id, slot, field)) = device_slot(expression) {
        let device = device(simulator, id)?;
        return device
            .slots
            .get(&slot)
            .and_then(|fields| fields.get(field))
            .copied()
            .map(Value::Number)
            .ok_or_else(|| format!("device `{id}` slot {slot} has no field `{field}`"));
    }
    if let Some((id, address)) = device_memory(expression) {
        let device = device(simulator, id)?;
        return device
            .memory
            .get(address)
            .copied()
            .map(Value::Number)
            .ok_or_else(|| format!("device `{id}` memory address {address} is out of range"));
    }
    if let Some((id, field)) = object(expression, "device") {
        return device(simulator, id)?
            .fields
            .get(field)
            .copied()
            .map(Value::Number)
            .ok_or_else(|| format!("device `{id}` has no field `{field}`"));
    }
    if let Some((id, field)) = object(expression, "network") {
        let index = simulator
            .world
            .network_index(id)
            .ok_or_else(|| format!("unknown network `{id}`"))?;
        let channel = channel_index(field).ok_or_else(|| format!("invalid channel `{field}`"))?;
        return Ok(Value::Number(
            simulator.world.networks[index].channels[channel],
        ));
    }
    if let Some(reference) = device_reference(simulator, thread, expression) {
        return Ok(Value::Text(reference));
    }
    if let Ok(value) = cpu.program.resolve_number(expression, &simulator.knowledge) {
        return Ok(Value::Number(value));
    }
    parse_number(expression).map(Value::Number)
}

pub fn set_value(
    simulator: &mut Simulator,
    thread: usize,
    target: &str,
    value: f64,
) -> Result<(), String> {
    let target = target.trim();
    if let Some(index) = direct_register_index(target) {
        let cpu = simulator
            .cpus
            .get_mut(thread)
            .ok_or_else(|| format!("unknown thread {}", thread + 1))?;
        cpu.registers[index] = value;
        return Ok(());
    }
    if let Some(address) = indexed(target, "stack[", "]") {
        let cpu = simulator
            .cpus
            .get_mut(thread)
            .ok_or_else(|| format!("unknown thread {}", thread + 1))?;
        let cell = cpu
            .stack
            .get_mut(address)
            .ok_or_else(|| format!("stack address {address} is out of range"))?;
        *cell = value;
        return Ok(());
    }
    if let Some((id, slot, field)) = device_slot(target) {
        let index = simulator
            .world
            .device_index(id)
            .ok_or_else(|| format!("unknown device `{id}`"))?;
        let cell = simulator.world.devices[index]
            .slots
            .get_mut(&slot)
            .and_then(|fields| fields.get_mut(field))
            .ok_or_else(|| format!("device `{id}` slot {slot} has no field `{field}`"))?;
        *cell = value;
        return Ok(());
    }
    if let Some((id, address)) = device_memory(target) {
        let index = simulator
            .world
            .device_index(id)
            .ok_or_else(|| format!("unknown device `{id}`"))?;
        let cell = simulator.world.devices[index]
            .memory
            .get_mut(address)
            .ok_or_else(|| format!("device `{id}` memory address {address} is out of range"))?;
        *cell = value;
        return Ok(());
    }
    if let Some((id, field)) = object(target, "device") {
        return simulator.set_device_field(id, field, value);
    }
    if let Some((id, field)) = object(target, "network") {
        let index = simulator
            .world
            .network_index(id)
            .ok_or_else(|| format!("unknown network `{id}`"))?;
        let channel = channel_index(field).ok_or_else(|| format!("invalid channel `{field}`"))?;
        simulator.world.networks[index].channels[channel] = value;
        return Ok(());
    }
    Err(format!(
        "`{target}` is not an assignable simulator location"
    ))
}

fn split_operator<'a>(
    expression: &'a str,
    operators: &[&str],
) -> Option<(&'a str, &'a str, &'a str)> {
    let bytes = expression.as_bytes();
    let mut depth = 0_i32;
    let mut quoted = false;
    let mut result = None;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'"' if index == 0 || bytes[index - 1] != b'\\' => quoted = !quoted,
            b'(' | b'[' if !quoted => depth += 1,
            b')' | b']' if !quoted => depth -= 1,
            _ => {}
        }
        if depth == 0 && !quoted {
            for operator in operators {
                if expression[index..].starts_with(operator) {
                    if (*operator == "-" || *operator == "+") && index == 0 {
                        continue;
                    }
                    result = Some((
                        &expression[..index],
                        &expression[index..index + operator.len()],
                        &expression[index + operator.len()..],
                    ));
                    index += operator.len().saturating_sub(1);
                    break;
                }
            }
        }
        index += 1;
    }
    result
}

fn function_call(expression: &str) -> Option<(&str, &str)> {
    let open = expression.find('(')?;
    if !expression.ends_with(')') {
        return None;
    }
    let name = expression[..open].trim();
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return None;
    }
    Some((name, &expression[open + 1..expression.len() - 1]))
}

fn validate_delimiters(expression: &str) -> Result<(), String> {
    let mut stack = Vec::new();
    let mut quoted = false;
    let mut escaped = false;
    for character in expression.chars() {
        if quoted {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                quoted = false;
            }
            continue;
        }
        match character {
            '"' => quoted = true,
            '(' | '[' => stack.push(character),
            ')' => {
                if stack.pop() != Some('(') {
                    return Err("unbalanced `)` in expression".to_owned());
                }
            }
            ']' => {
                if stack.pop() != Some('[') {
                    return Err("unbalanced `]` in expression".to_owned());
                }
            }
            _ => {}
        }
    }
    if quoted {
        return Err("unterminated string in expression".to_owned());
    }
    if let Some(character) = stack.pop() {
        return Err(format!("unclosed `{character}` in expression"));
    }
    Ok(())
}

fn trim_outer(mut expression: &str) -> &str {
    loop {
        if !expression.starts_with('(') || !expression.ends_with(')') {
            return expression;
        }
        let mut depth = 0_i32;
        let mut closes_at_end = false;
        for (index, byte) in expression.bytes().enumerate() {
            if byte == b'(' {
                depth += 1;
            }
            if byte == b')' {
                depth -= 1;
                if depth == 0 {
                    closes_at_end = index + 1 == expression.len();
                    break;
                }
            }
        }
        if !closes_at_end {
            return expression;
        }
        expression = expression[1..expression.len() - 1].trim();
    }
}

fn object<'a>(expression: &'a str, function: &str) -> Option<(&'a str, &'a str)> {
    let rest = expression.strip_prefix(function)?.strip_prefix("(\"")?;
    rest.split_once("\").")
}

fn device_slot(expression: &str) -> Option<(&str, usize, &str)> {
    let rest = expression.strip_prefix("device(\"")?;
    let (id, rest) = rest.split_once("\").slot[")?;
    let (slot, field) = rest.split_once("].")?;
    Some((id, slot.parse().ok()?, field))
}

fn device_memory(expression: &str) -> Option<(&str, usize)> {
    let rest = expression.strip_prefix("device(\"")?;
    let (id, address) = rest.split_once("\").memory[")?;
    Some((id, address.strip_suffix(']')?.parse().ok()?))
}

fn indexed(expression: &str, prefix: &str, suffix: &str) -> Option<usize> {
    expression
        .strip_prefix(prefix)?
        .strip_suffix(suffix)?
        .parse()
        .ok()
}

fn device<'a>(simulator: &'a Simulator, id: &str) -> Result<&'a ic10_sim::Device, String> {
    let index = simulator
        .world
        .device_index(id)
        .ok_or_else(|| format!("unknown device `{id}`"))?;
    Ok(&simulator.world.devices[index])
}

fn device_reference(simulator: &Simulator, thread: usize, expression: &str) -> Option<String> {
    let cpu = simulator.cpus.get(thread)?;
    let resolved = cpu.program.resolve_alias(expression);
    let (reference, connection) = resolved
        .split_once(':')
        .map_or((resolved, None), |(reference, connection)| {
            (reference, connection.parse::<usize>().ok())
        });
    let device_index = if reference == "db" {
        cpu.housing
    } else {
        let pin = reference.strip_prefix('d')?.parse::<usize>().ok()?;
        let Some(device) = cpu.pins.get(pin).copied().flatten() else {
            return Some(format!("{reference} · <not set>"));
        };
        device
    };
    let device = &simulator.world.devices[device_index];
    let mut description = format!("{} · {}", device.id, device.name);
    if let Some(connection) = connection {
        if let Some(network_index) = device.connections.get(&connection) {
            let network = &simulator.world.networks[*network_index];
            description.push_str(&format!(" · connection {connection} → {}", network.id));
        } else {
            description.push_str(&format!(" · connection {connection} not attached"));
        }
    }
    Some(description)
}

pub fn parse_number(value: &str) -> Result<f64, String> {
    match value.trim() {
        "NaN" | "nan" => Ok(f64::NAN),
        "pinf" | "Infinity" | "+Infinity" | "inf" | "+inf" => Ok(f64::INFINITY),
        "ninf" | "-Infinity" | "-inf" => Ok(f64::NEG_INFINITY),
        "-0" => Ok(-0.0),
        value => value
            .parse::<f64>()
            .map_err(|_| format!("cannot evaluate `{value}`")),
    }
}

pub fn format_number(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_owned()
    } else if value == f64::INFINITY {
        "Infinity".to_owned()
    } else if value == f64::NEG_INFINITY {
        "-Infinity".to_owned()
    } else if value == 0.0 && value.is_sign_negative() {
        "-0".to_owned()
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ic10_sim::Simulator;

    use super::{
        Value, evaluate, evaluate_with_changed, format_number, parse_number, validate_delimiters,
    };

    #[test]
    fn special_numbers_round_trip() {
        for text in ["NaN", "Infinity", "-Infinity", "-0"] {
            assert_eq!(format_number(parse_number(text).unwrap()), text);
        }
        assert_eq!(parse_number("pinf").unwrap(), f64::INFINITY);
        assert_eq!(parse_number("ninf").unwrap(), f64::NEG_INFINITY);
    }

    #[test]
    fn malformed_expression_delimiters_are_actionable() {
        assert_eq!(
            validate_delimiters("(r0 + 1").unwrap_err(),
            "unclosed `(` in expression"
        );
        assert_eq!(
            validate_delimiters("device(\"x).On").unwrap_err(),
            "unterminated string in expression"
        );
    }

    #[test]
    fn evaluator_uses_one_grammar_for_aliases_world_values_and_helpers() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../ic10-sim/tests/fixtures/multi-ic.ic10sim.json");
        let mut simulator = Simulator::from_scenario_path(&fixture).unwrap();
        simulator.cpus[1]
            .program
            .aliases
            .insert("request".to_owned(), "r0".to_owned());
        simulator.cpus[1].registers[0] = -42.0;

        assert_eq!(
            evaluate(&simulator, 1, "abs(request) + 2 % 2").unwrap(),
            Value::Number(42.0)
        );
        assert_eq!(
            evaluate(
                &simulator,
                1,
                "isfinite(device(\"sorter\").slot[0].Quantity) && !isnan(r0)"
            )
            .unwrap(),
            Value::Boolean(true)
        );
        assert_eq!(
            evaluate_with_changed(&simulator, 1, "changed(request)", &|name, value| {
                name == "request" && value == -42.0
            })
            .unwrap(),
            Value::Boolean(true)
        );
        assert_eq!(evaluate(&simulator, 1, "line").unwrap(), Value::Number(1.0));
        assert_eq!(
            evaluate(&simulator, 1, "runState").unwrap(),
            Value::Text("Ready".to_owned())
        );
    }
}
