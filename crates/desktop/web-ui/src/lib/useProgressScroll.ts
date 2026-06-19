import { useMemo } from 'react';
import {
  computeProgressScrollLayout,
  type ProgressScrollItem,
  type ProgressScrollLayout,
} from './progressScroll';

export function useProgressScrollLayout(
  items: readonly ProgressScrollItem[],
  maxRows: number,
): ProgressScrollLayout {
  return useMemo(() => computeProgressScrollLayout(items, maxRows), [items, maxRows]);
}
