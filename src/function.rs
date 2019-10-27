use super::db;
use chrono::prelude::*;
use diesel::pg::PgConnection;
use dotenv::dotenv;
use log::{debug, error};
use meval;
use serenity::client::Client;
use serenity::framework::Framework;
use serenity::model::channel::Message;
use serenity::model::id::UserId;
use serenity::model::misc::EmojiIdentifier;
use serenity::prelude::{Context, EventHandler};
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use threadpool::ThreadPool;
use std::io;
use std::io::{Error, ErrorKind};

struct DBFunctions {
    db_conn: Arc<Mutex<PgConnection>>,
}

pub trait ZubotsuFunction {
    type Error;
    fn dispatch(
        &mut self,
        context: Context,
        message: Message,
        threadpool: &ThreadPool,
    ) -> Result<(), Self::Error>;
}

impl ZubotsuFunction for DBFunctions {
    type Error = Box<dyn std::error::Error>;
    fn dispatch(
        &mut self,
        context: Context,
        message: Message,
        _threadpool: &ThreadPool,
    ) -> Result<(), Self::Error> {
        let conn = self.db_conn.clone();
        let message_text = message.content.to_lowercase();
        // karmabot +69 @(apply #(lube %) (your butt))
        if message_text.starts_with("karmabot") {
            let guild_id = message.guild_id;
            // let's get access to the db conn
            let locked_conn = conn.lock().unwrap();
            let command = message_text.split(' ').collect::<Vec<&str>>();
            // karmabot leaderboards
            // karmabot @(apply #(lube %) (your butt))
            if command.len() == 2 {
                if command[1] == "leaderboards" {
                    let users = db::leaderboards(&*locked_conn)?;

                    // do we want to move this formatting code out to a separate funtion
                    let format = users.iter().enumerate().map(|(index, karma_user)| {
                        // this is technically unsafe transform but due to knowledge about the id system of discord
                        // we can ignore this for now (until 2084)
                        let user_id = karma_user.user_id as u64;
                        let user = match UserId::to_user(UserId(user_id), &context) {
                            Err(e) => {
                                error!("unknown id {} {}", user_id, e); // we error message here but don't fail out
                                format!("unknown id {}", user_id)
                            }
                            Ok(user) => match guild_id {
                                Some(guild_id) => match user.nick_in(&context, guild_id) {
                                    Some(nick_name) => nick_name,
                                    None => user.name,
                                },
                                None => user.name,
                            },
                        };
                        let karma_amount = match karma_user.karma {
                            Some(karma_amount) => karma_amount,
                            None => 0,
                        };
                        format!("{}. {} : {}", index + 1, user, karma_amount)
                    });
                    // TODO: update this so that we collect using `.map(|s| &**s)`
                    // instead so we can borrow these strings
                    message.channel_id.say(
                        &context,
                        format!(
                            "High Scores \n{}",
                            format.collect::<Vec<String>>().join("\n")
                        ),
                    )?;

                // TODO: do we want to only have the ability to look at your own karma, or anyone's on the server
                } else if message.mentions.len() == 1 {
                    let karma = db::get_karma_for_id(&*locked_conn, message.mentions[0].id.0)?;
                    message.reply(&context, format!("Here's their karma {}", karma))?;
                }
            // karmabot 69 @(apply #(lube %) (your butt))
            } else if command.len() > 2 {
                let eval_expr = message_text
                    .trim_start_matches("karmabot ")
                    .replace(" ", "");
                let eval_expr = eval_expr.split_at(eval_expr.find("<@").unwrap()).0;
                if eval_expr == "" {
                    return Err(Box::new(io::Error::new(ErrorKind::InvalidData, "empty command")));
                } else {
                    let karma_amount = meval::eval_str(eval_expr) ?;                        
                        for mention in message.mentions {
                            match db::upsert_user_karma(&*locked_conn, mention.id.0, karma_amount as i32) {
                                Err(err) => error!("upsert db error: {}", err),
                                _ => debug!("added {} karma for {}", karma_amount as i32, mention.id.0),
                            };
                        }
                    }
                
            }
        }
        Ok(())
    }
}
