/** Mirror `deepseek_core::approval::ApprovalMode::config_implies_auto_approve`. */
export function autoApproveFromPolicy(approvalPolicy: string): boolean {
  return approvalPolicy.trim().toLowerCase() === 'auto';
}
