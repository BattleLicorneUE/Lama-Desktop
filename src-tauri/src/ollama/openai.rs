//! Traduction entre le format de messages d'Ollama (celui de toute l'application) et l'API compatible OpenAI du moteur local
//! (`llama-server`). Le reste du code ne voit pas la différence : il reçoit les mêmes `ChatChunk`.

use super::{ChatChunk, ChatMessage};
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};

/// Messages Ollama → messages OpenAI. Ollama n'identifie pas les appels d'outils ; OpenAI exige un `tool_call_id` : on
/// numérote les appels et on rattache chaque résultat au plus ancien appel qui l'attend.
pub fn to_messages(messages: &[ChatMessage]) -> Vec<Value> {
    let mut out = Vec::with_capacity(messages.len());
    let mut counter = 0usize;
    let mut waiting: VecDeque<String> = VecDeque::new();
    for m in messages {
        match m.role.as_str() {
            "assistant" => {
                let mut msg = json!({ "role": "assistant", "content": m.content });
                if let Some(calls) = m.tool_calls.as_ref().filter(|c| !c.is_empty()) {
                    let mut list = Vec::new();
                    for c in calls {
                        let id = format!("call_{counter}");
                        counter += 1;
                        waiting.push_back(id.clone());
                        let f = &c["function"];
                        let args = match &f["arguments"] {
                            Value::String(s) => s.clone(),
                            Value::Null => "{}".into(),
                            other => other.to_string(),
                        };
                        list.push(json!({ "id": id, "type": "function", "function": { "name": f["name"], "arguments": args } }));
                    }
                    msg["tool_calls"] = Value::Array(list);
                }
                out.push(msg);
            }
            "tool" => {
                let id = waiting.pop_front().unwrap_or_else(|| {
                    counter += 1;
                    format!("call_orphan_{counter}")
                });
                out.push(json!({ "role": "tool", "tool_call_id": id, "content": m.content }));
            }
            role => match m.images.as_ref().filter(|i| !i.is_empty()) {
                Some(images) => {
                    let mut parts = vec![json!({ "type": "text", "text": m.content })];
                    for img in images {
                        parts.push(json!({ "type": "image_url", "image_url": { "url": format!("data:image/png;base64,{img}") } }));
                    }
                    out.push(json!({ "role": role, "content": parts }));
                }
                None => out.push(json!({ "role": role, "content": m.content })),
            },
        }
    }
    out
}

/// Options d'Ollama → champs de la requête OpenAI. `num_ctx` n'a pas d'effet : la taille du contexte est fixée au lancement du moteur.
pub fn apply_options(body: &mut Value, think: Option<Value>, options: Option<Value>) {
    if let Some(Value::Object(o)) = options {
        for (from, to) in [("temperature", "temperature"), ("top_p", "top_p"), ("top_k", "top_k"), ("min_p", "min_p"), ("seed", "seed"), ("repeat_penalty", "repeat_penalty"), ("num_predict", "max_tokens")] {
            if let Some(v) = o.get(from).filter(|v| !v.is_null()) {
                body[to] = v.clone();
            }
        }
    }
    match think {
        Some(Value::Bool(b)) => body["chat_template_kwargs"] = json!({ "enable_thinking": b }),
        Some(Value::String(level)) => body["reasoning_effort"] = Value::String(level),
        _ => {}
    }
}

/// Lit le flux `data: {...}` ligne par ligne et le ramène à des `ChatChunk`. Les appels d'outils arrivent en fragments : ils sont
/// recollés et rendus d'un seul coup, dans le dernier morceau.
#[derive(Default)]
pub struct Stream {
    calls: BTreeMap<usize, (String, String)>,
    prompt_tokens: u64,
    eval_tokens: u64,
    finished: bool,
}

impl Stream {
    /// Une ligne du flux. `Some(chunk)` quand elle apporte du texte ou clôt la réponse.
    pub fn feed(&mut self, line: &str) -> Result<Option<ChatChunk>, serde_json::Error> {
        let line = line.trim();
        let Some(data) = line.strip_prefix("data:") else { return Ok(None) };
        let data = data.trim();
        if data == "[DONE]" {
            return Ok(Some(self.finish()));
        }
        if data.is_empty() {
            return Ok(None);
        }
        let v: Value = serde_json::from_str(data)?;
        if let Some(u) = v.get("usage").filter(|u| u.is_object()) {
            self.prompt_tokens = u["prompt_tokens"].as_u64().unwrap_or(self.prompt_tokens);
            self.eval_tokens = u["completion_tokens"].as_u64().unwrap_or(self.eval_tokens);
        }
        let choice = &v["choices"][0];
        let delta = &choice["delta"];
        for c in delta["tool_calls"].as_array().into_iter().flatten() {
            let slot = self.calls.entry(c["index"].as_u64().unwrap_or(0) as usize).or_default();
            if let Some(n) = c["function"]["name"].as_str() {
                slot.0.push_str(n);
            }
            if let Some(a) = c["function"]["arguments"].as_str() {
                slot.1.push_str(a);
            }
        }
        let content = delta["content"].as_str().unwrap_or("").to_string();
        let thinking = delta["reasoning_content"].as_str().or_else(|| delta["reasoning"].as_str()).unwrap_or("").to_string();
        if content.is_empty() && thinking.is_empty() {
            return Ok(None);
        }
        Ok(Some(ChatChunk { content, thinking, ..Default::default() }))
    }

    /// La fin de la réponse : une seule fois, même si le flux se termine sans `[DONE]`.
    pub fn finish(&mut self) -> ChatChunk {
        if self.finished {
            return ChatChunk::default();
        }
        self.finished = true;
        let tool_calls = std::mem::take(&mut self.calls)
            .into_values()
            .filter(|(name, _)| !name.is_empty())
            .map(|(name, args)| json!({ "function": { "name": name, "arguments": serde_json::from_str::<Value>(&args).unwrap_or_else(|_| json!({})) } }))
            .collect();
        ChatChunk { done: true, tool_calls, prompt_tokens: self.prompt_tokens, eval_tokens: self.eval_tokens, ..Default::default() }
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage { role: role.into(), content: content.into(), thinking: None, tool_calls: None, tool_name: None, images: None }
    }

    #[test]
    fn tool_results_are_linked_to_their_calls_in_order() {
        let mut a = msg("assistant", "");
        a.tool_calls = Some(vec![
            json!({ "function": { "name": "read_file", "arguments": { "path": "a.txt" } } }),
            json!({ "function": { "name": "list_dir", "arguments": {} } }),
        ]);
        let mut t1 = msg("tool", "contenu A");
        t1.tool_name = Some("read_file".into());
        let out = to_messages(&[msg("user", "salut"), a, t1, msg("tool", "liste")]);
        assert_eq!(out[1]["tool_calls"][0]["id"], "call_0");
        assert_eq!(out[1]["tool_calls"][1]["id"], "call_1");
        assert_eq!(out[1]["tool_calls"][0]["function"]["arguments"], "{\"path\":\"a.txt\"}", "OpenAI veut les arguments en texte JSON");
        assert_eq!((out[2]["tool_call_id"].as_str(), out[3]["tool_call_id"].as_str()), (Some("call_0"), Some("call_1")));
    }

    #[test]
    fn images_become_content_parts() {
        let mut u = msg("user", "Que vois-tu ?");
        u.images = Some(vec!["AAAA".into()]);
        let out = to_messages(&[u]);
        assert_eq!(out[0]["content"][0]["text"], "Que vois-tu ?");
        assert_eq!(out[0]["content"][1]["image_url"]["url"], "data:image/png;base64,AAAA");
    }

    #[test]
    fn options_and_thinking_are_mapped() {
        let mut body = json!({});
        apply_options(&mut body, Some(json!(false)), Some(json!({ "temperature": 0.3, "num_predict": 64, "num_ctx": 32768 })));
        assert_eq!(body["temperature"], 0.3);
        assert_eq!(body["max_tokens"], 64);
        assert!(body.get("num_ctx").is_none());
        assert_eq!(body["chat_template_kwargs"]["enable_thinking"], false);
        let mut b2 = json!({});
        apply_options(&mut b2, Some(json!("high")), None);
        assert_eq!(b2["reasoning_effort"], "high");
    }

    #[test]
    fn stream_text_reasoning_usage_and_tool_calls() {
        let mut s = Stream::default();
        let lines = [
            r#"data: {"choices":[{"delta":{"role":"assistant","content":""}}]}"#,
            r#"data: {"choices":[{"delta":{"reasoning_content":"Je réfléchis"}}]}"#,
            r#"data: {"choices":[{"delta":{"content":"Bon"}}]}"#,
            r#"data: {"choices":[{"delta":{"content":"jour"}}]}"#,
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"x","function":{"name":"read_","arguments":"{\"pa"}}]}}]}"#,
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"name":"file","arguments":"th\":\"a.txt\"}"}}]}}]}"#,
            r#"data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#,
            r#"data: {"choices":[],"usage":{"prompt_tokens":12,"completion_tokens":7}}"#,
            "data: [DONE]",
        ];
        let mut text = String::new();
        let mut thinking = String::new();
        let mut done = None;
        for l in lines {
            if let Some(c) = s.feed(l).unwrap() {
                text.push_str(&c.content);
                thinking.push_str(&c.thinking);
                if c.done {
                    done = Some(c);
                }
            }
        }
        assert_eq!((text.as_str(), thinking.as_str()), ("Bonjour", "Je réfléchis"));
        let d = done.unwrap();
        assert_eq!((d.prompt_tokens, d.eval_tokens), (12, 7));
        assert_eq!(d.tool_calls[0]["function"]["name"], "read_file");
        assert_eq!(d.tool_calls[0]["function"]["arguments"]["path"], "a.txt");
        assert!(!s.finish().done, "la fin n'est rendue qu'une fois");
    }

    #[test]
    fn broken_tool_arguments_become_an_empty_object_and_noise_is_ignored() {
        let mut s = Stream::default();
        assert!(s.feed(": keep-alive").unwrap().is_none());
        assert!(s.feed("").unwrap().is_none());
        s.feed(r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"name":"x","arguments":"{oups"}}]}}]}"#).unwrap();
        let d = s.finish();
        assert_eq!(d.tool_calls[0]["function"]["arguments"], json!({}));
    }
}
