import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { X, AlertCircle, Settings } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { IconButton } from "@/components/ui/IconButton";
import { useInstanceStore } from "@/stores/instanceStore";
import { ProviderType, CreateInstanceRequest } from "@/types";

interface CreateInstanceDialogProps {
  isOpen: boolean;
  onClose: () => void;
  onOpenSettings?: () => void;
}

export const CreateInstanceDialog = ({
  isOpen,
  onClose,
  onOpenSettings,
}: CreateInstanceDialogProps) => {
  const { t } = useTranslation();
  const { providers, loadProviders, createInstance } = useInstanceStore();

  const [name, setName] = useState("");
  const [selectedProvider, setSelectedProvider] =
    useState<ProviderType>("anthropic");
  const [model, setModel] = useState("");
  const [apiBaseUrl, setApiBaseUrl] = useState("");
  const [isCreating, setIsCreating] = useState(false);
  const [error, setError] = useState("");

  // Load providers when dialog opens
  useEffect(() => {
    if (isOpen) {
      loadProviders();
    }
  }, [isOpen, loadProviders]);

  // Update default model when provider changes
  useEffect(() => {
    const provider = providers.find((p) => p.id === selectedProvider);
    if (provider?.default_model) {
      setModel(provider.default_model);
    } else {
      setModel("");
    }
  }, [selectedProvider, providers]);

  const selectedProviderInfo = providers.find((p) => p.id === selectedProvider);
  const needsApiKey = selectedProviderInfo?.needs_api_key ?? false;
  const hasApiKey = selectedProviderInfo?.has_api_key ?? false;
  const suggestedModels = selectedProviderInfo?.suggested_models ?? [];

  const handleCreate = async () => {
    if (!name.trim()) {
      setError(t("ai_instances.name_required"));
      return;
    }

    if (!model.trim()) {
      setError(t("ai_instances.model_required"));
      return;
    }

    if (needsApiKey && !hasApiKey) {
      setError(t("ai_instances.api_key_required"));
      return;
    }

    setIsCreating(true);
    setError("");

    try {
      const request: CreateInstanceRequest = {
        name: name.trim(),
        provider: selectedProvider,
        model: model.trim(),
      };

      // Add api_base_url for Ollama or if custom URL is provided
      if (selectedProvider === "ollama" && apiBaseUrl.trim()) {
        request.api_base_url = apiBaseUrl.trim();
      }

      await createInstance(request);

      // Reset form
      setName("");
      setModel("");
      setApiBaseUrl("");
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsCreating(false);
    }
  };

  const handleOpenSettings = () => {
    onClose();
    onOpenSettings?.();
  };

  if (!isOpen) return null;

  return (
    <>
      {/* Backdrop */}
      <div
        className="fixed inset-0 bg-foreground/20 z-40 animate-slide-down"
        onClick={onClose}
      />

      {/* Dialog */}
      <div className="fixed inset-0 flex items-center justify-center z-50 p-4">
        <div className="bg-surface border border-border rounded-lg shadow-lg max-w-md w-full p-6 animate-slide-down">
          {/* Header */}
          <div className="flex items-center justify-between mb-6">
            <h2 className="text-xl font-serif">
              {t("ai_instances.create_new")}
            </h2>
            <IconButton icon={X} label={t("common.close")} onClick={onClose} />
          </div>

          {/* Content */}
          <div className="space-y-5">
            {/* Name Input */}
            <div>
              <label className="block text-sm font-medium text-muted mb-1.5">
                {t("ai_instances.name_label")}
              </label>
              <Input
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder={t("ai_instances.name_placeholder")}
                disabled={isCreating}
                autoFocus
              />
            </div>

            {/* Provider Selection */}
            <div>
              <label className="block text-sm font-medium text-muted mb-1.5">
                {t("ai_instances.provider_label")}
              </label>
              <div className="flex gap-2">
                {providers.map((provider) => (
                  <button
                    key={provider.id}
                    onClick={() => setSelectedProvider(provider.id)}
                    disabled={isCreating}
                    className={`
                      flex-1 py-2.5 px-3 rounded-md text-sm font-medium
                      border transition-colors
                      ${
                        selectedProvider === provider.id
                          ? "border-foreground bg-foreground text-background"
                          : "border-border hover:border-muted"
                      }
                      disabled:opacity-50
                    `}
                  >
                    {provider.name}
                  </button>
                ))}
              </div>
            </div>

            {/* API Key Warning */}
            {needsApiKey && !hasApiKey && (
              <div className="flex items-start gap-3 p-3 rounded-md bg-amber-50 border border-amber-200 text-amber-800">
                <AlertCircle className="w-5 h-5 shrink-0 mt-0.5" />
                <div className="flex-1 text-sm">
                  <p className="font-medium">
                    {t("ai_instances.api_key_missing")}
                  </p>
                  <p className="text-amber-700 mt-0.5">
                    {t("ai_instances.api_key_missing_hint")}
                  </p>
                  {onOpenSettings && (
                    <button
                      onClick={handleOpenSettings}
                      className="inline-flex items-center gap-1.5 mt-2 text-amber-900 hover:underline font-medium"
                    >
                      <Settings className="w-4 h-4" />
                      {t("ai_instances.open_settings")}
                    </button>
                  )}
                </div>
              </div>
            )}

            {/* Model Selection */}
            <div>
              <label className="block text-sm font-medium text-muted mb-1.5">
                {t("ai_instances.model_label")}
              </label>
              {suggestedModels.length > 0 ? (
                <div className="space-y-2">
                  <select
                    value={model}
                    onChange={(e) => setModel(e.target.value)}
                    disabled={isCreating}
                    className="
                      w-full px-3 py-2.5 rounded-md border border-border
                      bg-surface text-foreground
                      focus:outline-none focus:ring-2 focus:ring-foreground/20
                      disabled:opacity-50
                    "
                  >
                    {suggestedModels.map((m) => (
                      <option key={m} value={m}>
                        {m}
                      </option>
                    ))}
                    <option value="custom">
                      {t("ai_instances.model_custom")}
                    </option>
                  </select>
                  {model === "custom" && (
                    <Input
                      value=""
                      onChange={(e) => setModel(e.target.value)}
                      placeholder={t("ai_instances.model_custom_placeholder")}
                      disabled={isCreating}
                    />
                  )}
                </div>
              ) : (
                <Input
                  value={model}
                  onChange={(e) => setModel(e.target.value)}
                  placeholder={t("ai_instances.model_placeholder")}
                  disabled={isCreating}
                />
              )}
            </div>

            {/* API Base URL (for Ollama) */}
            {selectedProvider === "ollama" && (
              <div>
                <label className="block text-sm font-medium text-muted mb-1.5">
                  {t("ai_instances.api_base_url_label")}
                </label>
                <Input
                  value={apiBaseUrl}
                  onChange={(e) => setApiBaseUrl(e.target.value)}
                  placeholder="http://localhost:11434"
                  disabled={isCreating}
                />
                <p className="text-xs text-muted mt-1">
                  {t("ai_instances.api_base_url_hint")}
                </p>
              </div>
            )}

            {/* Error Display */}
            {error && <p className="text-sm text-red-600">{error}</p>}
          </div>

          {/* Actions */}
          <div className="flex items-center justify-end gap-3 mt-6">
            <Button variant="ghost" onClick={onClose} disabled={isCreating}>
              {t("common.cancel")}
            </Button>
            <Button
              onClick={handleCreate}
              isLoading={isCreating}
              disabled={
                !name.trim() ||
                !model.trim() ||
                (needsApiKey && !hasApiKey) ||
                isCreating
              }
            >
              {t("ai_instances.create")}
            </Button>
          </div>
        </div>
      </div>
    </>
  );
};
