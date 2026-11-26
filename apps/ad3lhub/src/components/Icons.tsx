type Props = { size?: number; className?: string };

export function IconPlay({ size = 16, className }: Props) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} aria-hidden>
      <path fill="currentColor" d="M4 2.5v11l9-5.5L4 2.5z" />
    </svg>
  );
}

export function IconDownload({ size = 16, className }: Props) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} aria-hidden>
      <path
        fill="currentColor"
        d="M8 2v7.2L5.4 6.6 4.3 7.7 8 11.4l3.7-3.7-1.1-1.1L9 9.2V2H8zm-5 10v1.5h10V12H3z"
      />
    </svg>
  );
}

export function IconFlame({ size = 16, className }: Props) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} aria-hidden>
      <path
        fill="currentColor"
        d="M8 1.5c.4 2.2-1.2 3.2-1.2 5.1 0 1 .6 1.8 1.5 2.1-.2-.8.2-1.7.9-2.2.8 1.4 1.8 2.4 1.8 4.1A4 4 0 0 1 8 14.5 4 4 0 0 1 4 10.4C4 7.2 6.2 5.8 8 1.5z"
      />
    </svg>
  );
}

export function IconFolder({ size = 16, className }: Props) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} aria-hidden>
      <path
        fill="currentColor"
        d="M2 3.5h4.2l1.3 1.3H14V13H2V3.5zm1 2.3v6.2h10V6.3H7.1L5.8 5H3v.8z"
      />
    </svg>
  );
}

export function IconGear({ size = 16, className }: Props) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} aria-hidden>
      <path
        fill="currentColor"
        d="M6.6 1.5h2.8l.3 1.6a5 5 0 0 1 1.2.7l1.5-.7 1.4 1.4-.7 1.5c.3.4.5.8.7 1.2l1.6.3v2.8l-1.6.3a5 5 0 0 1-.7 1.2l.7 1.5-1.4 1.4-1.5-.7a5 5 0 0 1-1.2.7l-.3 1.6H6.6l-.3-1.6a5 5 0 0 1-1.2-.7l-1.5.7-1.4-1.4.7-1.5a5 5 0 0 1-.7-1.2L1.5 9.4V6.6l1.6-.3c.2-.4.4-.8.7-1.2l-.7-1.5 1.4-1.4 1.5.7c.4-.3.8-.5 1.2-.7l.3-1.6zM8 6a2 2 0 1 0 0 4 2 2 0 0 0 0-4z"
      />
    </svg>
  );
}

export function IconUsers({ size = 16, className }: Props) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} aria-hidden>
      <path
        fill="currentColor"
        d="M6 2.5a2.2 2.2 0 1 1 0 4.4zm4.5 1a1.8 1.8 0 1 1 0 .2zM2.5 13c0-2.2 1.8-3.5 3.5-3.5S9.5 10.8 9.5 13v.5h-7V13zm7.2-2.8c.5-.2 1.1-.3 1.8-.3 1.7 0 3.5 1.1 3.5 3.1v.5h-2.5V13c0-.9-.3-1.7-.8-2.3-.3-.4-.7-.7-1.2-.9z"
      />
    </svg>
  );
}

export function IconTerminal({ size = 16, className }: Props) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} aria-hidden>
      <path
        fill="currentColor"
        d="M2 3h12v10H2V3zm1.5 1.5v7h9v-7h-9zm1.2 1.3 2.2 1.7-2.2 1.7.9 1.1L8 8.2 5.6 6.3l-.9 1.1zm4 4.2h3.5V11H8.7v-1z"
      />
    </svg>
  );
}

export function IconCheck({ size = 16, className }: Props) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} aria-hidden>
      <path
        fill="currentColor"
        d="M6.4 11.2 3.2 8l1.1-1.1 2.1 2.1 5.2-5.2L12.7 5 6.4 11.2z"
      />
    </svg>
  );
}

export function IconAlert({ size = 16, className }: Props) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} aria-hidden>
      <path
        fill="currentColor"
        d="M8 1.5 14.5 13H1.5L8 1.5zM7.2 6h1.6v3.2H7.2V6zm0 4.2h1.6V12H7.2v-1.8z"
      />
    </svg>
  );
}

export function IconLibrary({ size = 16, className }: Props) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} aria-hidden>
      <path fill="currentColor" d="M2 2h3v12H2V2zm4.5 0H14v12H6.5V2zM8 4v1.5h4.5V4H8zm0 3v1.5h4.5V7H8z" />
    </svg>
  );
}

export function IconRefresh({ size = 16, className }: Props) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" className={className} aria-hidden>
      <path
        fill="currentColor"
        d="M12.5 8A4.5 4.5 0 1 1 8 3.5V2l3 2.2L8 6.5V5A3 3 0 1 0 11 8h1.5z"
      />
    </svg>
  );
}
