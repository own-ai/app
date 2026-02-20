import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Trash2 } from "lucide-react";
import { cn } from "@/utils/cn";
import { Program } from "@/types";

interface ProgramListProps {
  programs: Program[];
  onSelect: (programName: string) => void;
  onDelete: (programName: string) => void;
}

export const ProgramList = ({
  programs,
  onSelect,
  onDelete,
}: ProgramListProps) => {
  const { t } = useTranslation();
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);

  const handleDelete = (programName: string) => {
    if (confirmDelete === programName) {
      onDelete(programName);
      setConfirmDelete(null);
    } else {
      setConfirmDelete(programName);
    }
  };

  if (programs.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center p-8">
        <p className="text-muted font-sans text-sm text-center">
          {t("canvas.no_programs")}
        </p>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto">
      <div className="p-4">
        <h3 className="text-sm font-sans font-medium text-muted mb-3 px-2">
          {t("canvas.program_list")}
        </h3>
        <ul className="space-y-1">
          {programs.map((program) => (
            <li key={program.id}>
              <div
                className={cn(
                  "group flex items-start gap-3 px-3 py-3 rounded-lg",
                  "hover:bg-surface cursor-pointer",
                  "transition-colors",
                )}
                onClick={() => onSelect(program.name)}
                role="button"
                tabIndex={0}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    onSelect(program.name);
                  }
                }}
              >
                <div className="flex-1 min-w-0">
                  <div className="flex items-baseline gap-2">
                    <span className="font-sans font-medium text-foreground truncate">
                      {program.name}
                    </span>
                    <span className="text-xs font-mono text-muted shrink-0">
                      {t("canvas.version", { version: program.version })}
                    </span>
                  </div>
                  {program.description && (
                    <p className="text-sm text-muted mt-0.5 truncate">
                      {program.description}
                    </p>
                  )}
                </div>

                {/* Delete button */}
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    handleDelete(program.name);
                  }}
                  className={cn(
                    "p-1.5 rounded shrink-0",
                    "text-muted hover:text-error",
                    "opacity-0 group-hover:opacity-100",
                    "transition-all",
                    confirmDelete === program.name && "opacity-100 text-error",
                  )}
                  aria-label={t("canvas.delete_program")}
                  title={
                    confirmDelete === program.name
                      ? t("canvas.confirm_delete", { name: program.name })
                      : t("canvas.delete_program")
                  }
                >
                  <Trash2 className="w-4 h-4" />
                </button>
              </div>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
};
