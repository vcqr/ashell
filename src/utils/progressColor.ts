/** 阶梯式进度条颜色（对齐 demo `uploadProgressColors`）。
 *
 *   ≤ 20  -> #f56c6c 红
 *   ≤ 40  -> #e6a23c 橙
 *   ≤ 60  -> #1989fa 蓝
 *   ≤ 80  -> #6f7ad3 紫
 *   ≤ 100 -> #5cb87a 绿
 */
export function progressColor(percent: number): string {
  if (percent <= 20) return "#f56c6c"
  if (percent <= 40) return "#e6a23c"
  if (percent <= 60) return "#1989fa"
  if (percent <= 80) return "#6f7ad3"
  return "#5cb87a"
}
