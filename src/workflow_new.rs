use crate::config::Config;
use crate::llm::{ChatMessage, LLMClient, parse_temperature_from_response};
use anyhow::Result;
use std::collections::HashMap;
use url::Url;

fn extract_domain_from_url(url: &str) -> Option<String> {
    Url::parse(url).ok().and_then(
