use serde::de::DeserializeOwned;

use chromiumoxide_cdp::cdp::js_protocol::runtime::{
    CallFunctionOnParams, EvaluateParams, RemoteObject,
};

use crate::utils::is_likely_js_function;

#[derive(Debug, Clone)]
pub struct EvaluationResult {
    /// Mirror object referencing original JavaScript object
    inner: RemoteObject,
}

impl EvaluationResult {
    pub fn new(inner: RemoteObject) -> Self {
        Self { inner }
    }

    pub fn object(&self) -> &RemoteObject {
        &self.inner
    }

    pub fn value(&self) -> Option<&serde_json::Value> {
        self.object().value.as_ref()
    }

    /// Attempts to deserialize the value into the given type
    pub fn into_value<T: DeserializeOwned>(self) -> serde_json::Result<T> {
        let value = self
            .inner
            .value
            .ok_or_else(|| serde::de::Error::custom("No value found"))?;
        serde_json::from_value(value)
    }
}

#[derive(Debug, Clone)]
pub enum Evaluation {
    Expression(EvaluateParams),
    Function(CallFunctionOnParams),
}

impl From<&str> for Evaluation {
    fn from(expression: &str) -> Self {
        if is_likely_js_function(expression) {
            CallFunctionOnParams::from(expression).into()
        } else {
            EvaluateParams::from(expression).into()
        }
    }
}

impl From<String> for Evaluation {
    fn from(expression: String) -> Self {
        expression.as_str().into()
    }
}

impl From<EvaluateParams> for Evaluation {
    fn from(params: EvaluateParams) -> Self {
        Evaluation::Expression(params)
    }
}

impl From<CallFunctionOnParams> for Evaluation {
    fn from(params: CallFunctionOnParams) -> Self {
        Evaluation::Function(params)
    }
}
