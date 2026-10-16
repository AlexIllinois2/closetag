import { HashRouter, Navigate, Route, Routes } from 'react-router-dom'
import { AppProvider, useApp } from './context'
import { useServerAlive } from './useServerAlive'
import ConfigPage from './components/ConfigPage'
import GroupsPage from './components/GroupsPage'
import Header from './components/Header'
import ItemsPage from './components/ItemsPage'
import TagsPage from './components/TagsPage'
import Toasts from './components/Toasts'
import './styles.css'

function Shell() {
  useServerAlive()
  const { repos } = useApp()
  return (
    <div className="app">
      <Header />
      {repos.length === 0 && (
        <div className="banner">
          还没有添加任何仓库 —— 到「配置」页添加 repo_paths，然后回来扫描、打标签。
        </div>
      )}
      <main className="main">
        <Routes>
          <Route path="/items" element={<ItemsPage />} />
          <Route path="/tags" element={<TagsPage />} />
          <Route path="/groups" element={<GroupsPage />} />
          <Route path="/config" element={<ConfigPage />} />
          <Route path="*" element={<Navigate to="/items" replace />} />
        </Routes>
      </main>
      <Toasts />
    </div>
  )
}

export default function App() {
  return (
    <AppProvider>
      <HashRouter>
        <Shell />
      </HashRouter>
    </AppProvider>
  )
}
