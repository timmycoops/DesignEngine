import { type ComponentProps, forwardRef } from 'react';
import { cn } from 'cn';

type Props = { value?: string; onChange?: (value: string) => void; label: string; size?: 'md' | 'sm' } & Omit<ComponentProps<'input'>, 'value' | 'onChange' | 'size'>;

/**
 * A box for words (Two sides plan, workstream 6): a name, a find, a value typed. In the kit's look, at the height of the
 * other boxes (`sm` for a row of a table or a bar), its label for a reader and in its tip. Without a `value` it holds
 * what's typed itself, from its `defaultValue`, for a box read when it's left.
 */
export const TextInput = forwardRef<HTMLInputElement, Props>(function TextInput({ value, onChange, label, size = 'md', className, title, ...props }, ref) {
  return (
    <input
      ref={ref}
      className={cn(
        'min-w-0 rounded-md border border-input bg-input/30 px-1.5 text-xs outline-none placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/50 disabled:opacity-50',
        size === 'md' ? 'h-7' : 'h-6 rounded-sm',
        className,
      )}
      value={value}
      aria-label={label}
      title={title ?? label}
      onChange={onChange && ((e) => onChange(e.target.value))}
      {...props}
    />
  );
});
