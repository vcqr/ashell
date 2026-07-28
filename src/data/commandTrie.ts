/**
 * 字典树（Trie）：前缀匹配的专用数据结构。
 *
 * - 插入 O(m)、删除 O(m)、前缀查找 O(m + k)，m = 键长，k = 匹配数
 * - 节点用 Map<string, TrieNode> 存子节点，支持任意 Unicode 字符
 * - 大小写不敏感：内部按 lower-case 建路径，终端节点保留原始命令名
 * - collect 做 DFS 遍历，结果按字符序自然有序
 */

interface TrieNode {
  children: Map<string, TrieNode>
  terminal: boolean
  /** 终端节点存储的原始命令名（保留大小写） */
  command: string
}

function createNode(): TrieNode {
  return { children: new Map(), terminal: false, command: "" }
}

export class CommandTrie {
  private root: TrieNode = createNode()

  insert(cmd: string): void {
    const lower = cmd.toLowerCase()
    let node = this.root
    for (const ch of lower) {
      let child = node.children.get(ch)
      if (!child) {
        child = createNode()
        node.children.set(ch, child)
      }
      node = child
    }
    node.terminal = true
    node.command = cmd
  }

  /** 删除指定命令。仅清除 terminal 标记，必要时回收无子节点的路径。 */
  delete(cmd: string): void {
    this.deleteRec(this.root, cmd.toLowerCase(), 0)
  }

  private deleteRec(node: TrieNode, key: string, depth: number): boolean {
    if (depth === key.length) {
      if (!node.terminal) return false
      node.terminal = false
      node.command = ""
      return node.children.size === 0
    }
    const ch = key[depth]
    if (!ch) return false
    const child = node.children.get(ch)
    if (!child) return false
    if (this.deleteRec(child, key, depth + 1)) {
      node.children.delete(ch)
      return !node.terminal && node.children.size === 0
    }
    return false
  }

  /**
   * 前缀查找：返回所有以 prefix 开头的命令。
   * 大小写不敏感。结果按字符序自然有序（DFS + Map 插入序）。
   */
  search(prefix: string): string[] {
    const lower = prefix.toLowerCase()
    let node = this.root
    for (const ch of lower) {
      const child = node.children.get(ch)
      if (!child) return []
      node = child
    }
    const results: string[] = []
    this.collect(node, results)
    return results
  }

  /** 判断是否存在以 prefix 开头的命令（不收集结果，比 search 更快）。 */
  hasPrefix(prefix: string): boolean {
    const lower = prefix.toLowerCase()
    let node = this.root
    for (const ch of lower) {
      const child = node.children.get(ch)
      if (!child) return false
      node = child
    }
    return true
  }

  /** DFS 收集子树中所有终端节点的命令。 */
  private collect(node: TrieNode, results: string[]): void {
    if (node.terminal) {
      results.push(node.command)
    }
    for (const child of node.children.values()) {
      this.collect(child, results)
    }
  }

  clear(): void {
    this.root = createNode()
  }

  /** 批量重建：清空后依次插入。 */
  rebuild(commands: Iterable<string>): void {
    this.clear()
    for (const cmd of commands) {
      this.insert(cmd)
    }
  }
}
