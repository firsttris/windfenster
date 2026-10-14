//! `windfenster admin`: the only way to make the first admin. Over the web nobody can make
//! themselves one; on the admin page an admin can name more.

use anyhow::{Result, bail};
use clap::Args;
use wf_core::admin::{log, users};
use wf_core::db::Db;

#[derive(Args)]
pub struct AdminArgs {
    #[command(flatten)]
    pub data: crate::DataArgs,
    /// The account: its e-mail address, its name, or `#id`
    pub who: Option<String>,
    /// Take the admin role instead of giving it
    #[arg(long)]
    pub remove: bool,
    /// List the admins
    #[arg(long, conflicts_with_all = ["who", "remove"])]
    pub list: bool,
}

pub async fn run(db: &Db, args: &AdminArgs) -> Result<()> {
    if args.list || args.who.is_none() {
        let admins = users::admins(db).await?;
        if admins.is_empty() {
            println!("no admins yet: windfenster admin <e-mail or name>");
        }
        for a in admins {
            println!("#{} {}{}", a.id, a.name, email_of(&a));
        }
        return Ok(());
    }
    let who = args.who.as_deref().unwrap_or_default();
    let user = match users::find(db, who).await? {
        users::Found::One(u) => u,
        users::Found::None => bail!("no account with the address or name {who:?}"),
        users::Found::Several(ids) => {
            let ids: Vec<String> = ids.iter().map(|i| format!("#{i}")).collect();
            bail!(
                "several accounts are called {who:?}; name one: {}",
                ids.join(", ")
            )
        }
    };
    let now = wf_core::time::now();
    if !users::set_admin(db, user.id, !args.remove).await? {
        bail!("{} is the last admin; make another one first", user.name);
    }
    let action = if args.remove {
        "role.revoke"
    } else {
        "role.grant"
    };
    log::add(db, now, None, action, Some(&user.name)).await?;
    println!(
        "{}{} {} admin",
        user.name,
        email_of(&user),
        if args.remove {
            "is no longer"
        } else {
            "is now"
        }
    );
    Ok(())
}

fn email_of(u: &users::AdminUser) -> String {
    u.email
        .as_deref()
        .map(|e| format!(" <{e}>"))
        .unwrap_or_default()
}
