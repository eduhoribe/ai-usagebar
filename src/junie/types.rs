use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

/// Normalized AI credits, using the conversion in Junie CLI 3893.6.
/// The account endpoint supplies neither a quota denominator nor a reset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub balance: f64,
    pub plan: String,
}

impl Eq for Snapshot {}

impl Snapshot {
    pub fn balance_label(&self) -> String {
        format!("{:.2} credits", self.balance)
    }

    pub fn validate(self) -> Result<Self> {
        if !self.balance.is_finite() {
            return Err(schema());
        }
        Ok(self)
    }
}

pub fn parse_response(bytes: &[u8]) -> Result<Snapshot> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Response {
        balance_left: f64,
        balance_unit: Option<String>,
        license_type: String,
    }
    let response: Response = serde_json::from_slice(bytes).map_err(|_| schema())?;
    // Junie intentionally hides GZL balances: they are not AI credits.
    if response.license_type == "GZL" {
        return Err(AppError::Schema(
            "Junie did not report an AI credit balance for this license".into(),
        ));
    }
    let balance = if response.balance_unit.as_deref() == Some("USD") {
        response.balance_left
    } else {
        response.balance_left / 100_000.0
    };
    let plan = match response.license_type.as_str() {
        "AIF" => "JetBrains AI Free",
        "AIP" => "JetBrains AI Pro",
        "AIPU" => "JetBrains AI Ultimate",
        "TRIAL" => "JetBrains Trial",
        "JUNP" => "Junie EAP",
        "NONE" => "No active license",
        _ => "Junie",
    };
    Snapshot {
        balance,
        plan: plan.into(),
    }
    .validate()
}

fn schema() -> AppError {
    AppError::Schema("Junie account response has no valid credit balance".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_only_balance_and_plan_with_the_cli_credit_conversion() {
        let s = parse_response(br#"{"balanceLeft":1250000,"licenseType":"AIPU","username":"private","token":"private"}"#).unwrap();
        assert_eq!(s.balance, 12.5);
        assert_eq!(s.plan, "JetBrains AI Ultimate");
        assert!(!serde_json::to_string(&s).unwrap().contains("private"));
        let s =
            parse_response(br#"{"balanceLeft":-0.5,"balanceUnit":"USD","licenseType":"TRIAL"}"#)
                .unwrap();
        assert_eq!(s.balance_label(), "-0.50 credits");
    }

    #[test]
    fn absent_or_invalid_balances_are_never_zero() {
        for bytes in [
            r#"{}"#,
            r#"{"balanceLeft":null,"licenseType":"AIP"}"#,
            r#"{"balanceLeft":"secret","licenseType":"AIP"}"#,
            r#"{"balanceLeft":1,"licenseType":"GZL"}"#,
            r#"{"balanceLeft":1e999,"licenseType":"AIP"}"#,
            "secret",
        ] {
            let error = parse_response(bytes.as_bytes()).unwrap_err().to_string();
            assert!(!error.contains("secret"));
        }
    }
}
