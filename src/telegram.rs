pub async fn send_message(token: &str, chat_id: &str, text: String) {
    if token == "YOUR_BOT_TOKEN_HERE" || token.is_empty() {
        println!("[TALON] Would have sent to Telegram:\n{}", text);
        return;
    }

    let client = reqwest::Client::new();
    let url = format!("https://api.telegram.org/bot{}/sendMessage", token);

    let _ = client.post(&url)
        .form(&[
            ("chat_id", chat_id),
            ("text", &text),
            ("parse_mode", "HTML"),
        ])
        .send()
        .await;
}
