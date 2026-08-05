import type { InputHTMLAttributes } from "react";
import { forwardRef } from "react";
import { cn } from "../utils";

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(
  function Input({ className, type = "text", ...props }, ref) {
    return (
      <input
        ref={ref}
        type={type}
        className={cn(
          "min-h-10 w-full rounded-md bg-surface-container-low px-3 font-sans text-ui leading-5 text-on-surface outline-none placeholder:text-on-surface-variant focus-visible:ring-1 focus-visible:ring-primary disabled:cursor-not-allowed disabled:opacity-60",
          className,
        )}
        {...props}
      />
    );
  },
);
