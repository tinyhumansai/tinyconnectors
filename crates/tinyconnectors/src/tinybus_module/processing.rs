//! Module-owned argument preparation and provider-error classification.
use crate::execute;
use tinyconnectors_bus::{PrepareArgumentsRequest, PreparedArguments, ProviderError};

pub(super) fn classified(tool: &str, message: &str) -> ProviderError {
    ProviderError {
        class: execute::classify_composio_error(tool, message)
            .as_str()
            .into(),
        message: execute::format_provider_error(tool, message),
    }
}

pub(super) fn parse_since(value: &str) -> tinybus::Result<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|date| date.with_timezone(&chrono::Utc))
        .map_err(|_| tinybus::Error::failed("invalid recency boundary; expected RFC3339"))
}

pub(super) fn prepare(request: PrepareArgumentsRequest) -> PreparedArguments {
    let mut arguments = request.arguments;
    if let Some(zone) = request.timezone {
        arguments = execute::apply_calendar_query_defaults(&request.tool, arguments, &zone);
    }
    if let Some(since) = request.since {
        let Ok(since) = parse_since(&since) else {
            return PreparedArguments {
                arguments: None,
                error: Some(ProviderError {
                    class: "validation".into(),
                    message: "invalid recency boundary; expected RFC3339".into(),
                }),
            };
        };
        arguments = execute::apply_window_args(&request.tool, arguments, since);
    }
    match execute::prepare_execute_arguments(&request.tool, arguments) {
        Ok(arguments) => PreparedArguments {
            arguments: Some(arguments),
            error: None,
        },
        Err(error) => PreparedArguments {
            arguments: None,
            error: Some(classified(&request.tool, &error.to_string())),
        },
    }
}
