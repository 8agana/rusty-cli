use crate::providers::ChatMessage;

// Very rough token estimator: ~4 chars per token + small overhead per message
pub fn estimate_tokens(text: &str) -> u32 {
    let chars = text.chars().count() as u32;
    let tokens = (chars / 4).max(1);
    tokens
}

pub fn estimate_messages_tokens(messages: &[ChatMessage]) -> u32 {
    let mut total = 0u32;
    for m in messages {
        total += 6; // overhead per message (very rough)
        total += estimate_tokens(&m.content);
    }
    total
}

pub fn trim_to_budget(messages: Vec<ChatMessage>, max_context_tokens: u32, reserve_output: u32) -> Vec<ChatMessage> {
    if max_context_tokens == 0 { return messages; }
    let budget = max_context_tokens.saturating_sub(reserve_output);

    // Separate pinned (leading system) and the rest
    let mut pinned: Vec<ChatMessage> = Vec::new();
    let mut rest: Vec<ChatMessage> = Vec::new();
    let mut iter = messages.into_iter();
    // Preserve the very first system message if present
    if let Some(first) = iter.next() {
        if first.role == "system" { pinned.push(first); } else { rest.push(first); }
    }
    rest.extend(iter);

    let pinned_cost: u32 = pinned.iter().map(|m| 6 + estimate_tokens(&m.content)).sum();
    if pinned_cost >= budget { return pinned; }

    let mut kept: Vec<ChatMessage> = Vec::new();
    let mut used = pinned_cost;
    for m in rest.iter().rev() {
        let cost = 6 + estimate_tokens(&m.content);
        if used + cost > budget { break; }
        kept.push(m.clone());
        used += cost;
    }
    kept.reverse();
    pinned.into_iter().chain(kept.into_iter()).collect()
}

