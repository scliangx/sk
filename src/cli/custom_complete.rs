//! 动态补全（clap-dyn-autocomplete）
//!
//! sk <Tab> → 返回已配置的服务器名称

use clap_dyn_autocomplete::{CustomCompleter, CustomCompleterFactory, RootCtx};
use crate::domain::config::store;

pub struct SkCompleterFactory;

impl CustomCompleterFactory for SkCompleterFactory {
    type CustomCompleter = SkCompleter;
    async fn build(&self, _ctx: &RootCtx<'_>) -> Self::CustomCompleter {
        SkCompleter
    }
}

pub struct SkCompleter;

impl CustomCompleter for SkCompleter {
    async fn complete(
        &self,
        _ctx: &RootCtx<'_>,
        _subcommand_path: &[&str],
        _arg_id: &str,
    ) -> Vec<String> {
        store::load_all().unwrap_or_default().into_iter().map(|s| s.name).collect()
    }
}
