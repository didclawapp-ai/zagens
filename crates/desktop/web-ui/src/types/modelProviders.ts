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
  /** Custom providers only: user-configured max_tokens cap (undefined = use default 65536) */
  max_output_tokens?: number | null;
  /** Catalog-backed model picker (OpenRouter, SenseNova, …). */
  has_catalog_picker: boolean;
}

export interface ProviderProbeResult {
  ok: boolean;
  message: string;
  models: string[] | null;
}

export type CatalogListVariant = 'flat' | 'free_paid';

export interface CatalogModelEntry {
  id: string;
  name: string;
  context_length?: number | null;
  max_output_length?: number | null;
  description?: string | null;
  is_free?: boolean | null;
}

export interface CatalogModelList {
  variant: CatalogListVariant;
  models: CatalogModelEntry[];
  free?: CatalogModelEntry[] | null;
  paid?: CatalogModelEntry[] | null;
  current_model: string | null;
  output_limits: Record<string, number>;
}
