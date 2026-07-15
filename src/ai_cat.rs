//! ИИ-пульс сцены через OpenRouter: раз в poll_secs дешёвая модель получает
//! контекст (погода, время, сезон) и возвращает настроение кота, его мысль
//! и афоризм для рекламного баннера за самолётом. Без ключа модуль молчит,
//! кот живёт обычной жизнью.

use serde::Deserialize;
use std::time::Duration;
use tokio::sync::mpsc;

const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

/// Настроение кота: смещает веса его состояний
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CatMood {
    Playful,
    Lazy,
    Hunty,
    Cozy,
}

impl CatMood {
    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "playful" => Some(Self::Playful),
            "lazy" => Some(Self::Lazy),
            "hunty" => Some(Self::Hunty),
            "cozy" => Some(Self::Cozy),
            _ => None,
        }
    }
}

/// Ответ модели: настроение, мысль кота, цитата для баннера
#[derive(Debug, Clone)]
pub struct AiPulse {
    pub mood: CatMood,
    pub thought: String,
    pub quote: String,
}

/// Контекст для промпта, собирается приложением
#[derive(Debug, Clone, Default)]
pub struct SceneContext {
    pub city: String,
    pub condition: String,
    pub temperature: f64,
    pub is_day: bool,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    content: String,
}

#[derive(Deserialize)]
struct PulseJson {
    mood: String,
    thought: String,
    quote: String,
}

/// Срезает markdown-обёртку ```json ... ``` если модель её добавила
fn strip_markdown(content: &str) -> &str {
    let trimmed = content.trim();
    let without_open = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    without_open.strip_suffix("```").unwrap_or(without_open).trim()
}

fn build_prompt(ctx: &SceneContext) -> String {
    let now = chrono::Local::now();
    let day_part = if ctx.is_day { "день" } else { "ночь" };
    format!(
        "Ты - рыжий кот, живущий во дворе дома на ASCII-скринсейвере в городе {city}. \
         Сейчас {time}, {date}, {day_part}. Погода: {condition}, {temp:.0}°C. \
         Ответь СТРОГО одним JSON-объектом без пояснений и markdown: \
         {{\"mood\":\"одно из: playful, lazy, hunty, cozy\",\
         \"thought\":\"короткая мысль кота от первого лица, до 35 символов, по-русски, без кавычек внутри\",\
         \"quote\":\"короткий житейский афоризм до 60 символов по-русски, без кавычек внутри\"}}",
        city = ctx.city,
        time = now.format("%H:%M"),
        date = now.format("%d.%m"),
        day_part = day_part,
        condition = ctx.condition,
        temp = ctx.temperature,
    )
}

async fn fetch_pulse(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    ctx: &SceneContext,
) -> Option<AiPulse> {
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": build_prompt(ctx)}],
        "max_tokens": 160,
    });

    let response: ChatResponse = client
        .post(OPENROUTER_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;

    let content = &response.choices.first()?.message.content;
    let parsed: PulseJson = serde_json::from_str(strip_markdown(content)).ok()?;

    let mut thought: String = parsed.thought.chars().take(38).collect();
    let mut quote: String = parsed.quote.chars().take(64).collect();
    thought = thought.replace(['"', '{', '}'], "");
    quote = quote.replace(['"', '{', '}'], "");

    Some(AiPulse {
        mood: CatMood::parse(&parsed.mood)?,
        thought,
        quote,
    })
}

/// Фоновый пульс: периодически спрашивает модель и шлёт результат.
/// Контекст сцены передаётся через watch-канал от приложения.
pub fn spawn_ai_watcher(
    api_key: String,
    model: String,
    poll_secs: u64,
    mut context_rx: tokio::sync::watch::Receiver<SceneContext>,
) -> mpsc::Receiver<AiPulse> {
    let (tx, rx) = mpsc::channel(1);

    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(45))
            .user_agent("weathr-screensaver")
            .build()
            .unwrap_or_default();

        // Первый пульс - вскоре после старта, когда погода уже пришла
        tokio::time::sleep(Duration::from_secs(20)).await;

        loop {
            let ctx = context_rx.borrow_and_update().clone();
            let mut ok = false;
            if !ctx.city.is_empty() || !ctx.condition.is_empty() {
                if let Some(pulse) = fetch_pulse(&client, &api_key, &model, &ctx).await {
                    if tx.send(pulse).await.is_err() {
                        break;
                    }
                    ok = true;
                }
            }
            // Неудача (сеть, парс) - повтор через минуту, не через весь период
            let delay = if ok { poll_secs.max(300) } else { 60 };
            tokio::time::sleep(Duration::from_secs(delay)).await;
        }
    });

    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_markdown() {
        assert_eq!(strip_markdown("```json\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(strip_markdown("{\"a\":1}"), "{\"a\":1}");
    }

    #[test]
    fn test_mood_parse() {
        assert_eq!(CatMood::parse("Lazy"), Some(CatMood::Lazy));
        assert_eq!(CatMood::parse("weird"), None);
    }
}
