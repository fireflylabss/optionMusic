import { BrowserRouter as Router, Navigate, Route, Routes } from 'react-router-dom'

import { ThemeProvider } from '@shared/components/theme/ThemeProvider'
import { Shell } from './components/layout/Shell'
import { HomePage } from './pages/HomePage'

function App() {
  return (
    <ThemeProvider>
      <Router>
        <Shell>
          <Routes>
            <Route path="/" element={<HomePage />} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Routes>
        </Shell>
      </Router>
    </ThemeProvider>
  )
}

export default App
