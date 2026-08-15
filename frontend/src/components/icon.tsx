export type IconName = 'grid' | 'route' | 'layers' | 'search' | 'plus' | 'spark' | 'folder' | 'chevron' | 'close' | 'back'

export function Icon({ name, size = 18 }: { name: IconName; size?: number }) {
  const paths: Record<IconName, string> = {
    grid: 'M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z',
    route: 'M5 5h14M5 12h14M5 19h14M8 3v4M16 10v4M10 17v4',
    layers: 'm12 3 9 5-9 5-9-5 9-5Zm-9 9 9 5 9-5M3 17l9 5 9-5',
    search: 'm20 20-4.5-4.5m2-5.5a7.5 7.5 0 1 1-15 0 7.5 7.5 0 0 1 15 0Z',
    plus: 'M12 5v14M5 12h14',
    spark: 'm12 3 1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9L12 3Zm7 13 .7 2.3L22 19l-2.3.7L19 22l-.7-2.3L16 19l2.3-.7L19 16Z',
    folder: 'M3 7h7l2 2h9v10H3V7Zm0 0V5h6l2 2',
    chevron: 'm8 10 4 4 4-4',
    close: 'm6 6 12 12M18 6 6 18',
    back: 'M15 6 9 12l6 6M9 12h12',
  }
  return (
    <svg aria-hidden="true" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">
      <path d={paths[name]} />
    </svg>
  )
}
