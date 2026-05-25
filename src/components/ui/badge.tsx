import * as React from "react";

import { cn } from "@/lib/utils";

const badgeVariantClasses = {
  default: "ui-badge--default",
  secondary: "ui-badge--secondary",
  outline: "ui-badge--outline",
  success: "ui-badge--success",
  destructive: "ui-badge--destructive",
} as const;

type BadgeVariant = keyof typeof badgeVariantClasses;

export interface BadgeProps
  extends React.HTMLAttributes<HTMLSpanElement> {
  variant?: BadgeVariant;
}

function Badge({ className, variant, ...props }: BadgeProps) {
  const variantClass = badgeVariantClasses[variant ?? "default"];
  return <span className={cn("ui-badge", variantClass, className)} {...props} />;
}

export { Badge };
