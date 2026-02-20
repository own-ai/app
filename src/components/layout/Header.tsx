import { useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { Menu, Settings, FolderOpen, PanelRight } from "lucide-react";
import { IconButton } from "@/components/ui/IconButton";
import { MenuDropdown } from "@/components/ui/MenuDropdown";
import { AIInstanceSelector } from "@/components/instances/AIInstanceSelector";
import { useInstanceStore } from "@/stores/instanceStore";
import { cn } from "@/utils/cn";

interface HeaderProps {
  onSettingsClick?: () => void;
  onCanvasToggle?: () => void;
  isCanvasOpen?: boolean;
  hasPrograms?: boolean;
}

export const Header = ({
  onSettingsClick,
  onCanvasToggle,
  isCanvasOpen = false,
  hasPrograms = false,
}: HeaderProps) => {
  const { t } = useTranslation();
  const { activeInstance } = useInstanceStore();
  const [isMenuOpen, setIsMenuOpen] = useState(false);

  const handleOpenWorkspace = async () => {
    if (!activeInstance) return;
    try {
      await invoke("open_workspace", { instanceId: activeInstance.id });
    } catch (error) {
      console.error("Failed to open workspace:", error);
    }
  };

  const menuItems = [
    {
      label: t("common.settings"),
      icon: Settings,
      onClick: onSettingsClick,
    },
  ];

  return (
    <header className="flex items-center justify-between px-5 py-4 border-b border-border bg-background sticky top-0 z-10">
      {/* AI Instance Selector */}
      <AIInstanceSelector />

      {/* Actions */}
      <div className="flex items-center gap-3">
        {activeInstance && (
          <IconButton
            icon={FolderOpen}
            label={t("common.open_workspace")}
            onClick={handleOpenWorkspace}
          />
        )}
        {(hasPrograms || isCanvasOpen) && (
          <IconButton
            icon={PanelRight}
            label={t("canvas.toggle_canvas")}
            onClick={onCanvasToggle}
            className={cn(isCanvasOpen && "text-accent")}
          />
        )}
        <div className="relative">
          <IconButton
            icon={Menu}
            label={t("common.menu")}
            onClick={() => setIsMenuOpen(!isMenuOpen)}
          />
          <MenuDropdown
            items={menuItems}
            isOpen={isMenuOpen}
            onClose={() => setIsMenuOpen(false)}
          />
        </div>
      </div>
    </header>
  );
};
