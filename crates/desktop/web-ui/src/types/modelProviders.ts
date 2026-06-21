export type ModelProviderSection = 'primary' | 'free' | 'custom';

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

export interface SenseNovaModelEntry {
  id: string;
  name: string;
  description: string | null;
  context_length: number | null;
  max_output_length: number | null;
}

export interface SenseNovaModelList {
  models: SenseNovaModelEntry[];
  current_model: string | null;
}
