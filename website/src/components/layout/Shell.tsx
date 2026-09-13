import { Footer } from './Footer'
import { Nav } from './Nav'

function Shell({ children }: { children: React.ReactNode }) {
  return (
    <>
      <Nav />
      <main className="w-full bg-background pb-24 pt-28 text-foreground">
        {children}
      </main>
      <Footer />
    </>
  )
}

export { Shell }
