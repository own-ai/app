import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { X, Key, Check, Trash2, Eye, EyeOff } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { IconButton } from "@/components/ui/IconButton";
import { useInstanceStore } from "@/stores/instanceStore";

interface SettingsProps {
  isOpen: boolean;
  onClose: () => void;
}

export const Settings = ({ isOpen, onClose }: SettingsProps) => {
  const { t } = useTranslation();
  const { providers, loadProviders, saveApiKey, deleteApiKey } =
    useInstanceStore();

  // Load providers when settings opens
  useEffect(() => {
    if (isOpen) {
      loadProviders();
    }
  }, [isOpen, loadProviders]);

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
        <div className="bg-surface border border-border rounded-lg shadow-lg max-w-lg w-full max-h-[90vh] overflow-hidden animate-slide-down">
          {/* Header */}
          <div className="flex items-center justify-between p-6 border-b border-border">
            <h2 className="text-xl font-serif">{t("settings.title")}</h2>
            <IconButton icon={X} label={t("common.close")} onClick={onClose} />
          </div>

          {/* Content */}
          <div className="p-6 overflow-y-auto max-h-[calc(90vh-80px)]">
            {/* API Keys Section */}
            <section>
              <h3 className="text-lg font-medium mb-4 flex items-center gap-2">
                <Key className="w-5 h-5" />
                {t("settings.api_keys")}
              </h3>
              <p className="text-sm text-muted mb-4">
                {t("settings.api_keys_description")}
              </p>

              <div className="space-y-4">
                {providers
                  .filter((p) => p.needs_api_key)
                  .map((provider) => (
                    <APIKeyRow
                      key={provider.id}
                      providerName={provider.name}
                      hasKey={provider.has_api_key}
                      onSave={(apiKey) => saveApiKey(provider.id, apiKey)}
                      onDelete={() => deleteApiKey(provider.id)}
                    />
                  ))}
              </div>
            </section>
          </div>
        </div>
      </div>
    </>
  );
};

interface APIKeyRowProps {
  providerName: string;
  hasKey: boolean;
  onSave: (apiKey: string) => Promise<void>;
  onDelete: () => Promise<void>;
}

const APIKeyRow = ({
  providerName,
  hasKey,
  onSave,
  onDelete,
}: APIKeyRowProps) => {
  const { t } = useTranslation();
  const [isEditing, setIsEditing] = useState(false);
  const [apiKey, setApiKey] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);
  const [error, setError] = useState("");

  const handleSave = async () => {
    if (!apiKey.trim()) {
      setError(t("settings.api_key_empty"));
      return;
    }

    setIsSaving(true);
    setError("");

    try {
      await onSave(apiKey.trim());
      setApiKey("");
      setIsEditing(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsSaving(false);
    }
  };

  const handleDelete = async () => {
    setIsDeleting(true);
    setError("");

    try {
      await onDelete();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsDeleting(false);
    }
  };

  const handleCancel = () => {
    setApiKey("");
    setIsEditing(false);
    setError("");
  };

  return (
    <div className="border border-border rounded-lg p-4">
      <div className="flex items-center justify-between mb-2">
        <span className="font-medium">{providerName}</span>
        {hasKey && !isEditing && (
          <span className="flex items-center gap-1.5 text-sm text-green-600">
            <Check className="w-4 h-4" />
            {t("settings.api_key_configured")}
          </span>
        )}
      </div>

      {isEditing ? (
        <div className="space-y-3">
          <div className="relative">
            <Input
              type={showKey ? "text" : "password"}
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder={t("settings.api_key_placeholder", {
                provider: providerName,
              })}
              disabled={isSaving}
              autoFocus
            />
            <button
              type="button"
              onClick={() => setShowKey(!showKey)}
              className="absolute right-3 top-1/2 -translate-y-1/2 text-muted hover:text-foreground"
            >
              {showKey ? (
                <EyeOff className="w-4 h-4" />
              ) : (
                <Eye className="w-4 h-4" />
              )}
            </button>
          </div>

          {error && <p className="text-sm text-red-600">{error}</p>}

          <div className="flex items-center gap-2">
            <Button
              onClick={handleSave}
              isLoading={isSaving}
              disabled={!apiKey.trim() || isSaving}
            >
              {t("common.save")}
            </Button>
            <Button variant="ghost" onClick={handleCancel} disabled={isSaving}>
              {t("common.cancel")}
            </Button>
          </div>
        </div>
      ) : (
        <div className="flex items-center gap-2">
          <Button variant="ghost" onClick={() => setIsEditing(true)}>
            {hasKey ? t("settings.api_key_change") : t("settings.api_key_add")}
          </Button>

          {hasKey && (
            <IconButton
              icon={Trash2}
              label={t("common.delete")}
              onClick={handleDelete}
              disabled={isDeleting}
              className="text-red-600 hover:text-red-700"
            />
          )}
        </div>
      )}
    </div>
  );
};

export default Settings;
