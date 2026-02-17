import { ButtonHTMLAttributes, forwardRef } from "react";
import { LucideIcon } from "lucide-react";
import { cn } from "@/utils/cn";

interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  icon: LucideIcon;
  label: string; // For accessibility
}

export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(
  ({ icon: Icon, label, className, ...props }, ref) => {
    return (
      <button
        ref={ref}
        className={cn(
          "p-2 rounded-lg",
          "text-muted hover:text-foreground",
          "hover:bg-surface",
          "transition-colors",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2",
          "disabled:opacity-50 disabled:cursor-not-allowed",
          className,
        )}
        aria-label={label}
        title={label}
        {...props}
      >
        <Icon className="w-5 h-5" />
      </button>
    );
  },
);

IconButton.displayName = "IconButton";
