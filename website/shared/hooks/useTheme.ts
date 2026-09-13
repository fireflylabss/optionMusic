import { useContext } from 'react'

import { ThemeContext } from '../components/theme/ThemeProvider'

function useTheme() {
  const ctx = useContext(ThemeContext)
  if (!ctx) throw new Error('useTheme must be used within ThemeProvider')
  return ctx
}

export { useTheme }
