import { createRoot } from 'react-dom/client'
import { Catalog } from './components/Catalog'
import { CommandPalette } from './components/CommandPalette'
import { ContextMenu } from './components/ContextMenu'
import { PlayerBar } from './components/PlayerBar'
import { SettingsPanel } from './components/SettingsPanel'
import { Sidebar } from './components/Sidebar'
import { Stage } from './components/Stage'
import { useOptMusic } from './hooks/useOptMusic'
import './styles.css'

function App() {
  const m = useOptMusic()
  const ldm = Boolean(m.view.settings.ldm)

  return (
    <div className={`shell${ldm ? ' ldm' : ''}`}>
      <Sidebar
        page={m.page}
        tracksCount={m.tracks.length}
        artistsCount={m.artists.length}
        favoritesCount={m.view.favorites.length}
        currentId={m.view.current?.id}
        recentTracks={m.recentTracks}
        goPage={m.goPage}
        play={m.play}
        openCommand={m.openCommand}
        addFolder={() => void m.addFolder()}
        openSettings={() => m.setSettingsOpen(true)}
      />

      <div className="workspace">
        <div className={`frame ${m.queueOpen ? 'with-queue' : ''}`}>
          <Stage
            current={m.view.current}
            playing={m.playing}
            coverSrc={m.coverSrc}
            progress={m.progress}
            position={m.clock.position}
            duration={m.clock.duration}
            favorited={m.favorited}
            queue={m.queue}
            queueOpen={m.queueOpen}
            setQueueOpen={m.setQueueOpen}
            toggle={m.toggle}
            toggleFavorite={m.toggleFavorite}
            seek={seconds => { void m.command('seek', { seconds }) }}
            play={m.play}
            removeFromQueue={id => { void m.command('queue_remove', { id }) }}
          />
          <Catalog
            page={m.page}
            pageTitle={m.pageTitle}
            loading={m.loading}
            error={m.error}
            status={m.status}
            tracks={m.tracks}
            visible={m.visible}
            artists={m.artists}
            albumsForArtist={m.albumsForArtist}
            artistSource={m.artistSource}
            artistKey={m.artistKey}
            albumKey={m.albumKey}
            selectedArtistName={m.selectedArtistName}
            playing={m.playing}
            current={m.view.current}
            favoriteIds={m.favoriteIds}
            focusedIndex={m.focusedIndex}
            queueOpen={m.queueOpen}
            queueLength={m.queue.length}
            locationCount={m.locationCount}
            listRef={m.listRef}
            setError={m.setError}
            setArtistKey={m.setArtistKey}
            setAlbumKey={m.setAlbumKey}
            setArtistMode={source => { void m.setArtistMode(source) }}
            setFocusedIndex={m.setFocusedIndex}
            setQueueOpen={m.setQueueOpen}
            openCommand={m.openCommand}
            loadLibrary={() => void m.loadLibrary()}
            addFolder={() => void m.addFolder()}
            play={m.play}
            addQueue={m.addQueue}
            toggleFavorite={m.toggleFavorite}
            openContext={m.openContext}
          />
        </div>

        <PlayerBar
          current={m.view.current}
          playing={m.playing}
          coverSrc={m.coverSrc}
          favorited={m.favorited}
          shuffled={m.view.shuffled}
          loopMode={m.view.loop_mode}
          muted={m.view.muted}
          volume={m.view.volume}
          excessVolume={Boolean(m.view.settings.excess_volume)}
          position={m.clock.position}
          duration={m.clock.duration}
          progress={m.progress}
          toggleFavorite={m.toggleFavorite}
          toggle={m.toggle}
          previous={m.previous}
          next={m.next}
          shuffle={() => void m.command('shuffle')}
          cycleLoop={() => void m.command('cycle_loop')}
          toggleMute={() => void m.command('toggle_mute')}
          setVolume={v => { void m.command('set_volume', { volume: v }) }}
          seek={seconds => { void m.command('seek', { seconds }) }}
        />
      </div>

      {m.commandOpen && (
        <CommandPalette
          search={m.search}
          setSearch={m.setSearch}
          hits={m.commandHits}
          commandIndex={m.commandIndex}
          setCommandIndex={m.setCommandIndex}
          currentId={m.view.current?.id}
          play={m.play}
          close={m.closeCommand}
        />
      )}
      {m.context && (
        <ContextMenu
          state={m.context}
          favorite={m.favoriteIds.has(m.context.track.id)}
          play={m.play}
          playNext={m.playNext}
          addQueue={m.addQueue}
          toggleFavorite={m.toggleFavorite}
        />
      )}
      {m.settingsOpen && (
        <SettingsPanel
          settings={m.view.settings}
          volume={m.view.volume}
          eq={m.view.eq}
          folders={m.folders}
          defaultMusicDir={m.defaultMusicDir}
          addFolder={() => void m.addFolder()}
          setVolume={v => { void m.command('set_volume', { volume: v }) }}
          setEq={eq => { void m.command('set_eq', { eq }) }}
          setExcess={v => { void m.command('set_excess_volume', { enabled: v }) }}
          setLdm={v => { void m.command('set_ldm', { enabled: v }) }}
          setArtistSource={source => { void m.setArtistMode(source) }}
          close={() => m.setSettingsOpen(false)}
        />
      )}
    </div>
  )
}

createRoot(document.getElementById('root')!).render(<App />)
