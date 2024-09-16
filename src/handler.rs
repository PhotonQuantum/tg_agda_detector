use eyre::Result;
use itertools::Itertools;
use sqlx::PgPool;
use std::hash::{DefaultHasher, Hash, Hasher};
use teloxide::macros::BotCommands;
use teloxide::payloads::SetMessageReactionSetters;
use teloxide::requests::Requester;
use teloxide::types::{
    InlineQuery, InlineQueryResult, InlineQueryResultArticle, InputMessageContent,
    InputMessageContentText, Message, ReactionType, UserId,
};
use teloxide::Bot;

const AGDA_CHARS: &[char] = &['喔', '哦'];

fn get_reaction(msg: &Message) -> Option<Vec<ReactionType>> {
    let text = msg.text()?;

    let text = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| {
            if AGDA_CHARS.contains(&c) {
                AGDA_CHARS[0]
            } else {
                c
            }
        })
        .collect::<String>();
    let Some((fst, snd)) = text.chars().take(2).collect_tuple() else {
        return Some(vec![]);
    };

    if fst.len_utf8() <= 1 || fst != snd {
        return Some(vec![]);
    }

    let emoji = if fst == AGDA_CHARS[0] { "🔥" } else { "🤔" };

    Some(vec![ReactionType::Emoji {
        emoji: emoji.to_string(),
    }])
}

pub async fn message_handler(msg: Message, bot: Bot, pool: PgPool) -> Result<()> {
    if let Some(reaction) = get_reaction(&msg) {
        if !reaction.is_empty() {
            sqlx::query!(
                "INSERT INTO logs (msg_id, user_id, chat_id, timestamp)\
                 VALUES ($1, $2, $3, current_timestamp)\
                 ON CONFLICT (msg_id, chat_id) DO NOTHING",
                msg.id.0,
                msg.from.map_or(0, |user| user.id.0) as i64,
                msg.chat.id.0
            )
            .execute(&pool)
            .await?;
            bot.set_message_reaction(msg.chat.id, msg.id)
                .reaction(reaction)
                .await?;
        }
    }
    Ok(())
}
pub async fn edit_message_handler(msg: Message, bot: Bot, pool: PgPool) -> Result<()> {
    if let Some(reaction) = get_reaction(&msg) {
        if reaction.is_empty() {
            sqlx::query!(
                "DELETE FROM logs WHERE msg_id = $1 AND chat_id = $2",
                msg.id.0,
                msg.chat.id.0
            )
            .execute(&pool)
            .await?;
        } else {
            sqlx::query!(
                "INSERT INTO logs (msg_id, user_id, chat_id, timestamp)\
                 VALUES ($1, $2, $3, current_timestamp)\
                 ON CONFLICT (msg_id, chat_id) DO NOTHING",
                msg.id.0,
                msg.from.map_or(0, |user| user.id.0) as i64,
                msg.chat.id.0
            )
            .execute(&pool)
            .await?;
        }
        bot.set_message_reaction(msg.chat.id, msg.id)
            .reaction(reaction)
            .await?;
    }
    Ok(())
}

pub async fn inline_handler(query: InlineQuery, bot: Bot, pool: PgPool) -> Result<()> {
    let user_agda_count = sqlx::query!(
        "SELECT COUNT(*) FROM logs WHERE user_id = $1",
        query.from.id.0 as i64
    )
    .fetch_one(&pool)
    .await?
    .count
    .unwrap_or(0);
    let user_today_agda_count = sqlx::query!(
        "SELECT COUNT(*) FROM logs WHERE user_id = $1 AND timestamp >= current_timestamp - interval '1 day'",
        query.from.id.0 as i64
    )
        .fetch_one(&pool)
        .await?
        .count.unwrap_or(0);

    let total_ans = format!("我已经阿鸽打了 {user_agda_count} 次了！");
    let today_ans = format!("我今天已经阿鸽打了 {user_today_agda_count} 次了！");
    let ans_list: [InlineQueryResult; 2] = [
        InlineQueryResultArticle::new(
            hash(&total_ans).to_string(),
            "阿鸽打总次数",
            InputMessageContent::Text(InputMessageContentText::new(total_ans)),
        )
        .description("查看你的总阿鸽打次数")
        .into(),
        InlineQueryResultArticle::new(
            hash(&today_ans).to_string(),
            "今日阿鸽打次数",
            InputMessageContent::Text(InputMessageContentText::new(today_ans)),
        )
        .description("查看你今天的阿鸽打次数")
        .into(),
    ];
    bot.answer_inline_query(query.id, ans_list).await?;

    Ok(())
}

fn hash(m: impl Hash) -> u64 {
    let mut s = DefaultHasher::new();
    m.hash(&mut s);
    s.finish()
}

#[derive(BotCommands, Clone)]
#[command(rename_rule = "snake_case", description = "支持以下命令:")]
pub enum Command {
    #[command(description = "显示帮助信息.")]
    Help,
    #[command(description = "查看阿鸽打统计.")]
    Stats,
}

pub async fn group_stat_command(msg: Message, bot: Bot, pool: PgPool) -> Result<()> {
    if !msg.chat.is_group() && !msg.chat.is_supergroup() {
        bot.send_message(msg.chat.id, "阿鸽打统计只能在群组和超级群组中使用。")
            .await?;
        return Ok(());
    }

    let total_agda_count = sqlx::query!(
        "SELECT COUNT(*) FROM logs WHERE chat_id = $1",
        msg.chat.id.0
    )
    .fetch_one(&pool)
    .await?
    .count
    .unwrap_or(0);
    let today_agda_count = sqlx::query!(
        "SELECT COUNT(*) FROM logs WHERE chat_id = $1 AND timestamp >= current_timestamp - interval '1 day'",
        msg.chat.id.0
    )
        .fetch_one(&pool)
        .await?
        .count.unwrap_or(0);

    let agda_scoreboard = sqlx::query!(
        "SELECT user_id, COUNT(*) AS count FROM logs WHERE chat_id = $1 AND timestamp >= current_timestamp - interval '1 day' GROUP BY user_id ORDER BY count DESC LIMIT 5",
        msg.chat.id.0
    )
        .fetch_all(&pool)
        .await?;
    let msg_scoreboard = futures::future::try_join_all(agda_scoreboard.iter().map(|row| async {
        let user_id = row.user_id as u64;
        let user = bot.get_chat_member(msg.chat.id, UserId(user_id)).await?;
        Ok::<_, eyre::Report>(format!(
            "{} - {} 次",
            user.user.full_name(),
            row.count.expect("count")
        ))
    }))
    .await?
    .join("\n");

    let ans = format!(
        "群友们总共阿鸽打了 {today_agda_count} 次，\
    今天已经阿鸽打了 {total_agda_count} 次！\n\n\
    今日阿鸽打排行榜：\n{msg_scoreboard}",
    );
    bot.send_message(msg.chat.id, ans).await?;

    Ok(())
}
