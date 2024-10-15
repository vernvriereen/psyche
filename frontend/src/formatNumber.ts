export function formatNumber(num: number): string {
  const suffixes = ["", "k", "m", "b", "t", "q"];
  const suffixThresholds = [1, 1e3, 1e6, 1e9, 1e12, 1e15];

  if (num < 0) {
    return `-${formatNumber(-num)}`;
  }

  if (num < 1000) {
    return num.toString();
  }

  let suffixIndex = suffixes.length - 1;
  while (suffixIndex > 0 && num < suffixThresholds[suffixIndex]) {
    suffixIndex--;
  }

  const scaledNum = num / suffixThresholds[suffixIndex];
  const roundedNum = Math.floor(scaledNum * 10) / 10;

  return roundedNum.toFixed(0) + suffixes[suffixIndex];
}
