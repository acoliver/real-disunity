use super::Request;
use crate::DisunityWorld;
use serde::Deserialize;
use serde_json::Value;

pub(crate) const MAX_TRANSACTION_OPERATIONS: usize = 128;

#[derive(Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub(crate) enum Transaction {
    #[serde(rename = "transaction")]
    Apply { operations: Vec<Value> },
}
impl Transaction {
    pub(super) fn changes_content(&self) -> Result<bool, String> {
        let Self::Apply { operations } = self;
        let mut changes = false;
        for value in operations {
            changes |= parse_operation(value.clone())?.changes_content()?;
        }
        Ok(changes)
    }

    pub(super) fn execute(
        self,
        world: &mut DisunityWorld,
        identity: u64,
        limit: usize,
    ) -> Result<Value, String> {
        let Self::Apply { operations } = self;
        if operations.is_empty() || operations.len() > MAX_TRANSACTION_OPERATIONS {
            return Err("transaction requires 1..128 operations".into());
        }
        let parsed = operations
            .into_iter()
            .map(parse_operation)
            .collect::<Result<Vec<Request>, String>>()?;
        let mut results = Vec::with_capacity(parsed.len());
        let mut budget = crate::encoding::Counter {
            limit,
            bytes: serde_json::to_vec(&super::envelope(Ok(Value::Array(Vec::new()))))
                .map_err(|error| error.to_string())?
                .len(),
        };
        for request in parsed {
            let result = request.execute_with_limit(world, identity, limit)?;
            budget.bytes += usize::from(!results.is_empty());
            serde_json::to_writer(&mut budget, &result)
                .map_err(|_| crate::response::RESPONSE_BUDGET_EXCEEDED.to_string())?;
            results.push(result);
        }
        Ok(Value::Array(results))
    }
}

fn parse_operation(value: Value) -> Result<Request, String> {
    if value["op"] == "transaction" {
        return Err("nested transactions are not supported".into());
    }
    // Suboperations have no envelopes, identities or phase credentials.
    let request: Request = serde_json::from_value(value).map_err(|error| error.to_string())?;
    if request.is_linear() {
        return Err("strict controls cannot be transacted".into());
    }
    Ok(request)
}
