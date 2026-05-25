import * as React from "react";

import { cn } from "@/lib/utils";

const buttonVariantClasses = {
  default: "ui-button--default",
  secondary: "ui-button--secondary",
  outline: "ui-button--outline",
  ghost: "ui-button--ghost",
  destructive: "ui-button--destructive",
} as const;

const buttonSizeClasses = {
  default: "ui-button--default-size",
  sm: "ui-button--sm",
  lg: "ui-button--lg",
  icon: "ui-button--icon",
} as const;

type ButtonVariant = keyof typeof buttonVariantClasses;
type ButtonSize = keyof typeof buttonSizeClasses;

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
}

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, type = "button", ...props }, ref) => {
    const variantClass = buttonVariantClasses[variant ?? "default"];
    const sizeClass = buttonSizeClasses[size ?? "default"];

    return (
      <button
        ref={ref}
        type={type}
        className={cn("ui-button", variantClass, sizeClass, className)}
        {...props}
      />
    );
  },
);

Button.displayName = "Button";

export { Button };
