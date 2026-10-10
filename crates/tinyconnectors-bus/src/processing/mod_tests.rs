//! Processing DTOs round-trip without linking implementation dependencies.
use super::*;

#[test]
fn optional_context_and_structured_errors_keep_their_wire_fields() -> Result<(), serde_json::Error>
{
    let request = PrepareArgumentsRequest {
        tool: "fixture".into(),
        ..Default::default()
    };
    let wire = serde_json::to_value(request)?;
    assert_eq!(
        wire,
        serde_json::json!({"tool":"fixture", "arguments":null, "timezone":null, "since":null})
    );
    let decoded: PrepareArgumentsRequest = serde_json::from_value(wire)?;
    assert_eq!(decoded.tool, "fixture");
    let error = ProviderError {
        class: "validation".into(),
        message: "fixture".into(),
    };
    let wire = serde_json::to_value(&error)?;
    assert_eq!(
        wire,
        serde_json::json!({"class":"validation","message":"fixture"})
    );
    assert_eq!(serde_json::from_value::<ProviderError>(wire)?, error);
    Ok(())
}
