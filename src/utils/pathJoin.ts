/** 用 / 作为分隔符规范化路径，去重重复斜杠 */
export function normalizePath(p: string): string {
  if (!p) return '/'
  const out = p.replace(/\\/g, '/').replace(/\/+/g, '/')
  if (out.length > 1 && out.endsWith('/')) return out.slice(0, -1)
  return out || '/'
}

export function joinPath(base: string, name: string): string {
  if (!base || base === '/') return normalizePath(`/${name}`)
  return normalizePath(`${base}/${name}`)
}

/** 取 path 的父目录，根目录返回 "/" */
export function parentPath(p: string): string {
  const n = normalizePath(p)
  if (n === '/' || n === '') return '/'
  const idx = n.lastIndexOf('/')
  if (idx <= 0) return '/'
  return n.slice(0, idx) || '/'
}

/** 拆分成面包屑段，每一段附带其完整路径（用于点击跳转） */
export interface PathCrumb {
  label: string
  path: string
}

export function pathCrumbs(p: string): PathCrumb[] {
  const n = normalizePath(p)
  if (n === '/' || n === '') return [{ label: '/', path: '/' }]
  const parts = n.split('/').filter(Boolean)
  const crumbs: PathCrumb[] = []
  let acc = ''
  for (const part of parts) {
    acc += `/${part}`
    crumbs.push({ label: part, path: acc })
  }
  return crumbs
}
