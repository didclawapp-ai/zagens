use anyhow::Result;

use crate::cli::args::ModelsArgs;
use crate::cli::context::CliContext;
use crate::client::DeepSeekClient;

pub async fn run(ctx: &CliContext, args: ModelsArgs) -> Result<()> {
    let client = DeepSeekClient::new(&ctx.config)?;
    let mut models = client.list_models().await?;
    models.sort_by(|a, b| a.id.cmp(&b.id));

    if args.json {
        println!("{}", serde_json::to_string_pretty(&models)?);
        return Ok(());
    }

    if models.is_empty() {
        println!("No models returned by the API.");
        return Ok(());
    }

    let default_model = ctx.config.default_model();
    println!("Available models (default: {default_model})");
    for model in models {
        let marker = if model.id == default_model { "*" } else { " " };
        if let Some(owner) = model.owned_by {
            println!("{marker} {} ({owner})", model.id);
        } else {
            println!("{marker} {}", model.id);
        }
    }

    Ok(())
}
