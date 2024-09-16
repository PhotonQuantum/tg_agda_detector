mod axum_listener;
mod handler;

use crate::handler::{edit_message_handler, group_stat_command, message_handler, Command};
use eyre::Result;
use futures::future::BoxFuture;
use sqlx::migrate::Migrator;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::env;
use std::fmt::Debug;
use std::str::FromStr;
use std::sync::Arc;
use teloxide::dispatching::{Dispatcher, UpdateFilterExt};
use teloxide::error_handlers::ErrorHandler;
use teloxide::requests::Requester;
use teloxide::types::{Message, Update};
use teloxide::utils::command::BotCommands;
use teloxide::{dptree, update_listeners, Bot};
use tracing::warn;

static MIGRATOR: Migrator = sqlx::migrate!();

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();

    color_eyre::install()?;

    tracing_subscriber::fmt::init();

    warn!("Starting agda detector bot");

    let url = env::var("BOT_SERVER").unwrap_or_else(|_| {
        option_env!("BOT_SERVER")
            .unwrap_or("https://api.telegram.org")
            .to_string()
    });
    let bot = Bot::from_env().set_api_url(url.parse().expect("Parse telegram bot api url error."));

    let pg_opts =
        PgConnectOptions::from_str(&env::var("DATABASE_URL").expect("DATABASE_URL must be set"))
            .expect("DATABASE_URL must be a valid PG connection string");
    let pg_opts = if let Ok(password) = env::var("DATABASE_PASSWORD") {
        pg_opts.password(&password)
    } else {
        pg_opts
    };
    let pgpool = PgPoolOptions::new()
        .connect_with(pg_opts)
        .await
        .expect("Failed to connect to database");
    MIGRATOR
        .run(&pgpool)
        .await
        .expect("Failed to run migrations");

    let command_handler = teloxide::filter_command::<Command, _>()
        .branch(
            dptree::case![Command::Help].endpoint(|msg: Message, bot: Bot| async move {
                bot.send_message(msg.chat.id, Command::descriptions().to_string())
                    .await?;
                Ok(())
            }),
        )
        .branch(dptree::case![Command::Stats].endpoint(group_stat_command));
    let mut dp = Dispatcher::builder(
        bot.clone(),
        dptree::entry()
            .branch(
                Update::filter_message()
                    .branch(command_handler)
                    .endpoint(message_handler),
            )
            .branch(Update::filter_edited_message().endpoint(edit_message_handler))
            .branch(Update::filter_inline_query().endpoint(handler::inline_handler)),
    )
    .dependencies(dptree::deps![pgpool])
    .enable_ctrlc_handler()
    .build();
    if let (Ok(base), Ok(path), Ok(addr)) = (
        env::var("APP_WEBHOOK_URL"),
        env::var("APP_WEBHOOK_PATH"),
        env::var("APP_BIND_ADDR"),
    ) {
        Box::pin(
            dp.dispatch_with_listener(
                axum_listener::axum(
                    bot,
                    update_listeners::webhooks::Options::new(
                        addr.parse().expect("invalid bind address"),
                        base.parse().expect("invalid base url"),
                    )
                    .path(path),
                )
                .await
                .expect("failed to start webhook"),
                Arc::new(TracingErrorHandler),
            ),
        )
        .await;
    } else {
        Box::pin(dp.dispatch_with_listener(
            update_listeners::polling_default(bot).await,
            Arc::new(TracingErrorHandler),
        ))
        .await;
    }

    Ok(())
}
struct TracingErrorHandler;

impl<E> ErrorHandler<E> for TracingErrorHandler
where
    E: Debug,
{
    fn handle_error(self: Arc<Self>, error: E) -> BoxFuture<'static, ()> {
        tracing::error!("Error occur from update listener: {:?}", error);

        Box::pin(async {})
    }
}
