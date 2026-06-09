//! Tool execution context and MCP pool wiring.

use super::*;

impl Engine {
    pub(super) fn build_tool_context(&self, mode: AppMode, auto_approve: bool) -> ToolContext {
        // Load the per-workspace trusted-paths list (#29) on every tool-context
        // build. Cheap (a small JSON file) and always reflects the latest
        // `/trust add` / `/trust remove` mutations without an explicit cache
        // refresh hook.
        let trusted = crate::workspace_trust::WorkspaceTrust::load_for(&self.session.workspace);
        let mut trusted_paths = trusted.paths().to_vec();
        for root in crate::skills::trusted_skill_roots(&self.session.workspace) {
            if !trusted_paths
                .iter()
                .any(|existing| crate::tools::spec::path_has_prefix(existing, &root))
            {
                trusted_paths.push(root);
            }
        }
        let mut ctx = ToolContext::with_auto_approve(
            self.session.workspace.clone(),
            self.session.trust_mode,
            self.session.notes_path.clone(),
            self.session.mcp_config_path.clone(),
            mode == AppMode::Yolo || auto_approve,
        )
        .with_state_namespace(self.session.id.clone())
        .with_features(self.config.features.clone())
        .with_shell_manager(self.runtime_ext().shell_manager.clone())
        .with_runtime_services(self.config_ext().runtime_services.clone())
        .with_cancel_token(self.cancel_token.clone())
        .with_trusted_external_paths(trusted_paths);

        // Hand the user-memory path to tools so the model-callable
        // `remember` tool can append entries (#489). `None` when the
        // feature is disabled — tools short-circuit on that.
        if self.config.memory_enabled {
            ctx.memory_path = Some(self.config.memory_path.clone());
        }

        if let Some(decider) = self.config_ext().network_policy.as_ref() {
            ctx = ctx.with_network_policy(decider.clone());
        }

        // Wire the search provider so the web_search tool uses the configured backend.
        ctx = ctx.with_search_config(
            self.config_ext().search_provider.clone(),
            self.config_ext().search_api_key.clone(),
        );

        // Wire the large-output router (#548). Only attaches when the
        // [workshop] config table is present; sub-agents don't inherit the
        // router (their ToolContext is built separately) to prevent recursive
        // routing of the synthesis call itself.
        if let Some(workshop_cfg) = self.config_ext().workshop.as_ref()
            && let Some(vars_arc) = self.runtime_ext().workshop_vars.as_ref()
        {
            let router =
                crate::tools::large_output_router::LargeOutputRouter::new(workshop_cfg.clone());
            ctx = ctx.with_large_output_router(router, vars_arc.clone());
        }

        // Wire the external sandbox backend (#516). exec_shell checks this
        // field and routes commands through the backend instead of spawning
        // a local process when it's set.
        if let Some(backend) = self.sandbox.backend() {
            ctx = ctx.with_sandbox_backend(std::sync::Arc::clone(backend));
        }

        if self.config.scratchpad.enabled {
            ctx = ctx.with_audit_scratchpad_run_id(self.scratchpad_run_id.clone());
        }
        ctx = ctx.with_subagent_default_step_timeout_ms(
            self.config.subagent_step_timeout.as_millis() as u64,
        );

        // Sub-agents clone this ToolContext via `SubAgentRuntime::child_runtime`;
        // wire LSP here so `diagnostics` and post-edit hooks work in child turns.
        let lsp_manager = &self.runtime_ext().lsp_manager;
        if lsp_manager.config().enabled {
            ctx = ctx.with_lsp_manager(std::sync::Arc::clone(lsp_manager));
        }

        // Honour the user's `sandbox_mode` config when it is *stricter* than
        // the AppMode default. The AppMode floor still applies: YOLO always
        // gets DangerFullAccess, and Plan stays at the default (no shell).
        let mode_policy = match mode {
            AppMode::Plan => return ctx,
            AppMode::Agent => crate::sandbox::SandboxPolicy::WorkspaceWrite {
                writable_roots: vec![self.session.workspace.clone()],
                network_access: true,
                exclude_tmpdir: false,
                exclude_slash_tmp: false,
            },
            AppMode::Yolo => crate::sandbox::SandboxPolicy::DangerFullAccess,
        };

        let effective = if let Some(raw) = self.config.sandbox_mode.as_deref() {
            if let Some(user_policy) = crate::sandbox::SandboxPolicy::parse_from_config(raw) {
                // Use whichever is more restrictive (higher level).
                if user_policy.restriction_level() > mode_policy.restriction_level() {
                    user_policy
                } else {
                    mode_policy
                }
            } else {
                mode_policy
            }
        } else {
            mode_policy
        };

        ctx.with_elevated_sandbox_policy(effective)
    }

    pub(super) async fn ensure_mcp_pool(&mut self) -> Result<Arc<AsyncMutex<McpPool>>, ToolError> {
        if let Some(pool) = self.runtime_ext().mcp_pool.clone() {
            return Ok(pool);
        }

        let pool = if let Some(shared) = crate::mcp_shared::shared_mcp_pool() {
            shared
        } else {
            let network_policy = self.config_ext().network_policy.clone();
            let mut pool =
                McpPool::from_config_path(&self.session.mcp_config_path).map_err(|e| {
                    ToolError::execution_failed(format!("Failed to load MCP config: {e}"))
                })?;
            if let Some(decider) = network_policy.as_ref() {
                pool = pool.with_network_policy(decider.clone());
            }
            Arc::new(AsyncMutex::new(pool))
        };

        self.runtime_ext_mut().mcp_pool = Some(Arc::clone(&pool));
        Ok(pool)
    }

    pub(super) async fn mcp_tools(&mut self) -> Vec<Tool> {
        let pool = match self.ensure_mcp_pool().await {
            Ok(pool) => pool,
            Err(err) => {
                let _ = self.tx_event.send(Event::status(err.to_string())).await;
                return Vec::new();
            }
        };

        let mut pool = pool.lock().await;
        let errors = pool.connect_all().await;
        for (server, err) in errors {
            let _ = self
                .tx_event
                .send(Event::status(format!(
                    "Failed to connect MCP server '{server}': {err}"
                )))
                .await;
        }

        pool.to_api_tools()
    }
}
