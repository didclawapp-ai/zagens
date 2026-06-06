//! Topic extraction and emotion detection.

use regex::Regex;

use crate::graph::EmotionMode;
use crate::stopwords::is_stop_word;

const MAX_TOPICS_PER_TURN: usize = 6;

fn emotion_patterns() -> &'static [(EmotionMode, &'static [&'static str])] {
    &[
        (
            EmotionMode::Angry,
            &[
                r"！{2,}",
                r"[草操艹]",
                r"(?:烦死|气死|蠢|傻|垃圾|什么破|搞什么|凭什么)",
                r"(?i)(?:fuck|damn|shit|wtf|stupid|idiot|ridiculous|annoying|hate)",
                r"[A-Z]{4,}",
            ],
        ),
        (
            EmotionMode::Happy,
            &[
                r"哈{2,}",
                r"(?:666|牛[啊哦!！]?|太好了|太棒了|完美|太爽|嘿嘿|耶)",
                r"(?i)(?:awesome|great|excellent|amazing|wonderful|perfect|haha|lol|yay|nice)",
                r"(?:可以了|正好|搞定了|成了|终于)",
            ],
        ),
        (
            EmotionMode::Sad,
            &[
                r"(?:唉|哎|呜|唔)[^哈]*",
                r"(?:算了|没意思|好累|烦躁|郁闷|难过|不想|放弃|搞不定|不知道咋办)",
                r"\.{3,}|…{2,}",
                r"(?i)(?:sigh|tired|frustrated|depressed|sad|whatever|meh|hopeless)",
                r"(?:怎么办|没有用|没用|失败了|又失败)",
            ],
        ),
    ]
}

/// Detect dominant emotion from user text (≥2 signal hits for A/B/C, else N).
#[must_use]
pub fn detect_emotion(text: &str) -> EmotionMode {
    let trimmed = text.trim();
    if trimmed.len() < 2 {
        return EmotionMode::Neutral;
    }
    let mut scores = [
        (EmotionMode::Angry, 0u32),
        (EmotionMode::Happy, 0),
        (EmotionMode::Sad, 0),
    ];
    for (mode, patterns) in emotion_patterns() {
        for pat in *patterns {
            if let Ok(re) = Regex::new(pat)
                && re.is_match(trimmed)
                && let Some((_, score)) = scores.iter_mut().find(|(m, _)| *m == *mode)
            {
                *score += 1;
            }
        }
    }
    for (mode, score) in scores {
        if score >= 2 {
            return mode;
        }
    }
    EmotionMode::Neutral
}

/// Extract topic tokens from text (Chinese 2–6 char segments + English words ≥3).
#[must_use]
pub fn extract_topics(text: &str) -> Vec<String> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let mut cleaned = text.to_string();
    if let Ok(re) = Regex::new(r"```[\s\S]*?```") {
        cleaned = re.replace_all(&cleaned, " ").to_string();
    }
    let cleaned = Regex::new(r"`[^`]+`")
        .ok()
        .map(|re| re.replace_all(&cleaned, " ").to_string())
        .unwrap_or(cleaned);
    let cleaned = Regex::new(r"https?://\S+")
        .ok()
        .map(|re| re.replace_all(&cleaned, " ").to_string())
        .unwrap_or(cleaned);
    let cleaned = Regex::new(r"[#*_~>|\[\]()]+")
        .ok()
        .map(|re| re.replace_all(&cleaned, " ").to_string())
        .unwrap_or(cleaned);
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");

    let mut freq: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

    if let Ok(cn_re) = Regex::new(r"[\u{4e00}-\u{9fff}\u{3400}-\u{4dbf}]{2,6}") {
        for cap in cn_re.find_iter(&cleaned) {
            let w = cap.as_str();
            if !is_stop_word(w) {
                *freq.entry(w.to_string()).or_default() += 1;
            }
        }
    }

    if let Ok(en_re) = Regex::new(r"[a-zA-Z]{3,}") {
        for cap in en_re.find_iter(&cleaned) {
            let lw = cap.as_str().to_ascii_lowercase();
            if !is_stop_word(&lw) {
                *freq.entry(lw).or_default() += 1;
            }
        }
    }

    let mut entries: Vec<_> = freq.into_iter().collect();
    entries.sort_by_key(|b| std::cmp::Reverse(b.1));
    entries
        .into_iter()
        .take(MAX_TOPICS_PER_TURN)
        .map(|(w, _)| w)
        .collect()
}

/// Topics where the user expressed confusion / knowledge gaps.
#[must_use]
pub fn detect_blocked_topics(user_text: &str) -> Vec<String> {
    let patterns = [
        r"不(?:知道|懂|了解|明白|清楚)(.{2,10})",
        r"(?:不太|完全不|没有)(?:了解|明白|理解)(.{2,10})",
        r"(?i)(?:i don'?t know|i'?m not sure about|don'?t understand)\s+(.{3,30})",
        r"(?i)(?:what is|what are|explain)\s+(.{3,30})",
    ];
    let mut blocked = Vec::new();
    for pat in patterns {
        if let Ok(re) = Regex::new(pat) {
            for cap in re.captures_iter(user_text) {
                if let Some(m) = cap.get(1) {
                    let topic = m
                        .as_str()
                        .trim()
                        .trim_end_matches(['？', '?', '。', '，', ',', '！', '!']);
                    if !topic.is_empty() && !is_stop_word(topic) {
                        blocked.push(topic.to_string());
                    }
                }
            }
        }
    }
    blocked
}
