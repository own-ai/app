import { useEffect, useRef } from "react";
import { cn } from "@/utils/cn";

interface MenuItem {
  label: string;
  icon?: React.ComponentType<{ className?: string }>;
  onClick?: () => void;
}

interface MenuDropdownProps {
  items: MenuItem[];
  isOpen: boolean;
  onClose: () => void;
  className?: string;
}

export const MenuDropdown = ({
  items,
  isOpen,
  onClose,
  className,
}: MenuDropdownProps) => {
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!isOpen) return;

    const handleClickOutside = (event: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
        onClose();
      }
    };

    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onClose();
      }
    };

    // Delay adding listener to avoid immediate close from the triggering click
    requestAnimationFrame(() => {
      document.addEventListener("mousedown", handleClickOutside);
      document.addEventListener("keydown", handleEscape);
    });

    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      document.removeEventListener("keydown", handleEscape);
    };
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  return (
    <div
      ref={menuRef}
      className={cn(
        "absolute right-0 top-full mt-2 min-w-48 py-1",
        "bg-background border border-border rounded-md shadow-lg",
        "z-50",
        className,
      )}
    >
      {items.map((item, index) => (
        <button
          key={index}
          className={cn(
            "w-full flex items-center gap-3 px-4 py-2.5 text-sm text-left",
            "text-foreground hover:bg-muted/10 transition-colors",
          )}
          onClick={() => {
            item.onClick?.();
            onClose();
          }}
        >
          {item.icon && <item.icon className="w-4 h-4 text-muted" />}
          <span>{item.label}</span>
        </button>
      ))}
    </div>
  );
};
