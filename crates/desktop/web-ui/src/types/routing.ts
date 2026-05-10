/** Routing rule: intent → model mapping */
export interface RoutingRule {
  intent: string;
  model: string;
}

export interface RoutingRulesResponse {
  rules: RoutingRule[];
}
