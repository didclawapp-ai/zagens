/** Mirror `deepseek_core::approval::ApprovalMode::config_implies_auto_approve`. */
export function autoApproveFromPolicy(approvalPolicy: string): boolean {
  return approvalPolicy.trim().toLowerCase() === 'auto';
}

/** Composer may override runtime auto-approve only when config policy is explicitly `auto`. */
export function composerAutoApproveToggleEnabled(approvalPolicy: string): boolean {
  return autoApproveFromPolicy(approvalPolicy);
}

/** Maps `approval_policy` config value → settings i18n key suffix. */
export function approvalPolicySettingsKey(
  approvalPolicy: string,
): 'approvalAuto' | 'approvalOnRequest' | 'approvalUntrusted' | 'approvalNever' {
  switch (approvalPolicy.trim().toLowerCase()) {
    case 'auto':
      return 'approvalAuto';
    case 'never':
    case 'deny':
      return 'approvalNever';
    case 'untrusted':
      return 'approvalUntrusted';
    default:
      return 'approvalOnRequest';
  }
}
