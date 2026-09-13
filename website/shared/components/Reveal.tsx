import { useEffect, useRef } from 'react'
import type { CSSProperties, ReactNode } from 'react'

import { cn } from '../lib/utils'

function Reveal({
  children,
  className,
  delay = 0,
}: {
  children: ReactNode
  className?: string
  delay?: number
}) {
  const ref = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    const el = ref.current
    if (!el) return
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      el.classList.add('option-revealed')
      return
    }
    const io = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            el.classList.add('option-revealed')
            io.disconnect()
          }
        }
      },
      { threshold: 0.15, rootMargin: '0px 0px -8% 0px' },
    )
    io.observe(el)
    return () => io.disconnect()
  }, [])

  const style: CSSProperties | undefined = delay
    ? { transitionDelay: `${delay}ms` }
    : undefined

  return (
    <div ref={ref} className={cn('option-reveal', className)} style={style}>
      {children}
    </div>
  )
}

export { Reveal }
