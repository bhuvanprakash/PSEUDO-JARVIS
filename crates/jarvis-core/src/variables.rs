use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct VariableContext {
    values: HashMap<String, String>,
}

impl VariableContext {
    pub fn new(run_id: &str) -> Self {
        let mut values = HashMap::new();
        let now = chrono::Utc::now();
        values.insert("run_id".into(), run_id.into());
        values.insert("date".into(), now.format("%Y-%m-%d").to_string());
        values.insert("year".into(), now.format("%Y").to_string());
        values.insert("month".into(), now.format("%m").to_string());
        values.insert("timestamp".into(), now.to_rfc3339());
        Self { values }
    }

    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.values.insert(key.into(), value.into());
    }

    pub fn set_step_output(&mut self, step_id: &str, key: &str, value: impl Into<String>) {
        self.values
            .insert(format!("step.{step_id}.{key}"), value.into());
    }

    pub fn interpolate(&self, input: &str) -> String {
        let mut output = input.to_string();
        for (key, value) in &self.values {
            let pattern = format!("{{{{ {key} }}}}");
            let pattern_tight = format!("{{{{{key}}}}}");
            output = output.replace(&pattern, value);
            output = output.replace(&pattern_tight, value);
        }
        output
    }

    pub fn interpolate_json(&self, value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::String(s) => serde_json::Value::String(self.interpolate(s)),
            serde_json::Value::Array(arr) => {
                serde_json::Value::Array(arr.iter().map(|v| self.interpolate_json(v)).collect())
            }
            serde_json::Value::Object(map) => serde_json::Value::Object(
                map.iter()
                    .map(|(k, v)| (k.clone(), self.interpolate_json(v)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolates_variables() {
        let ctx = VariableContext::new("abc");
        assert_eq!(ctx.interpolate("run {{ run_id }}"), "run abc");
    }
}
