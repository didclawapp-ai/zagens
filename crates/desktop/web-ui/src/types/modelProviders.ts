export type ModelProviderSection = 'primary' | 'free';

export interface ModelProviderStatus {
  id: string;
  display_name: string;
  section: ModelProviderSection;
  configured: boolean;
  active: boolean;
  key_required: boolean;
  model: string | null;
  base_url: string | null;
  service_ok: boolean | null;
  service_detail: string | null;
}

export interface ProviderProbeResult {
  ok: boolean;
  message: string;
  models: string[] | null;
}

export interface OpenRouterModelEntry {
  id: string;
  name: string;
  is_free: boolean;
}

export interface OpenRouterModelList {
  free: OpenRouterModelEntry[];
  paid: OpenRouterModelEntry[];
  current_model: string | null;
}
