import * as React from "react";
import { Card, CardContent } from "@/components/ui/card";
import { cn } from "@/lib/utils";

export const settingsCardClass = "rounded-lg border bg-card p-4 elevation-flat";

type SettingsCardProps = React.HTMLAttributes<HTMLDivElement> & {
  children: React.ReactNode;
};

export function SettingsCard({ className, children, ...props }: SettingsCardProps) {
  return (
    <div className={cn(settingsCardClass, className)} {...props}>
      {children}
    </div>
  );
}

type SettingsCardHeaderProps = {
  title: React.ReactNode;
  description?: React.ReactNode;
  action?: React.ReactNode;
  className?: string;
};

export function SettingsCardHeader({
  title,
  description,
  action,
  className,
}: SettingsCardHeaderProps) {
  return (
    <div className={cn(description ? "mb-4" : "mb-3", className)}>
      <div
        className={cn(
          "flex gap-2",
          action ? "items-start justify-between" : "items-center",
        )}
      >
        <div className="min-w-0">
          <h3 className="text-sm font-medium">{title}</h3>
          {description && (
            <p className="text-xs text-muted-foreground mt-1">{description}</p>
          )}
        </div>
        {action && <div className="shrink-0">{action}</div>}
      </div>
    </div>
  );
}

type SettingSectionProps = React.HTMLAttributes<HTMLDivElement> & {
  children: React.ReactNode;
};

export function SettingSection({ className, children, ...props }: SettingSectionProps) {
  return (
    <Card className={cn("elevation-flat", className)} {...props}>
      <CardContent className="p-4">{children}</CardContent>
    </Card>
  );
}

type SettingRowProps = React.HTMLAttributes<HTMLDivElement> & {
  icon?: React.ReactNode;
  title: React.ReactNode;
  description?: React.ReactNode;
  action?: React.ReactNode;
};

export function SettingRow({
  icon,
  title,
  description,
  action,
  className,
  children,
  ...props
}: SettingRowProps) {
  return (
    <div className={cn("flex items-center justify-between gap-4", className)} {...props}>
      <div className="min-w-0 space-y-0.5">
        <div className="flex items-center gap-2">
          {icon}
          <span className="text-sm font-medium">{title}</span>
        </div>
        {description && (
          <p className="text-xs leading-5 text-muted-foreground">{description}</p>
        )}
        {children}
      </div>
      {action && <div className="shrink-0">{action}</div>}
    </div>
  );
}
