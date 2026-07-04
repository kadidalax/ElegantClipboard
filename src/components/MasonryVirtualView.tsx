import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import { Separator } from "@/components/ui/separator";
import {
  getCachedMasonryHeight,
  setCachedMasonryHeight,
} from "@/lib/masonry-height-cache";
import {
  buildMasonrySection,
  estimateClipboardItemHeight,
  findPlacedItemByIndex,
  getVisiblePlacedItems,
  MASONRY_COLUMN_GAP_PX,
  MASONRY_HORIZONTAL_INSET_PX,
  MASONRY_SEPARATOR_HEIGHT_PX,
  MASONRY_VIRTUAL_OVERSCAN_PX,
  masonryColumnWidth,
  masonryRowGapPx,
  type MasonryPlacedItem,
} from "@/lib/masonry-layout";
import type { ClipboardItem } from "@/stores/clipboard";

export interface MasonryVirtualViewHandle {
  scrollToIndex: (index: number, behavior?: ScrollBehavior) => void;
}

interface MasonryVirtualViewProps<T extends ClipboardItem> {
  items: T[];
  pinnedCount: number;
  cardDensity: string;
  cardMaxLines: number;
  imageMaxHeight: number;
  imageAutoHeight: boolean;
  scrollParent: HTMLElement;
  renderCard: (item: T, index: number) => ReactNode;
}

function MasonryMeasuredSlot<T extends ClipboardItem>({
  item,
  index,
  onHeightChange,
  style,
  children,
}: {
  item: T;
  index: number;
  onHeightChange: (itemId: number, height: number) => void;
  style: CSSProperties;
  children: ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const element = ref.current;
    if (!element) return;

    const report = () => {
      const height = element.getBoundingClientRect().height;
      if (height > 0) onHeightChange(item.id, height);
    };

    report();
    const observer = new ResizeObserver(report);
    observer.observe(element);
    return () => observer.disconnect();
  }, [item.id, onHeightChange]);

  return (
    <div ref={ref} data-clipboard-index={index} style={style} className="list-item-enter">
      {children}
    </div>
  );
}

function renderPlacedItem<T extends ClipboardItem>(
  entry: MasonryPlacedItem<T>,
  columnWidth: number,
  sectionOffset: number,
  onHeightChange: (itemId: number, height: number) => void,
  renderCard: (item: T, index: number) => ReactNode,
) {
  return (
    <MasonryMeasuredSlot
      key={`${entry.index}-${entry.item.id}`}
      item={entry.item}
      index={entry.index}
      onHeightChange={onHeightChange}
      style={{
        position: "absolute",
        top: sectionOffset + entry.top,
        left:
          MASONRY_HORIZONTAL_INSET_PX +
          entry.column * (columnWidth + MASONRY_COLUMN_GAP_PX),
        width: columnWidth,
      }}
    >
      {renderCard(entry.item, entry.index)}
    </MasonryMeasuredSlot>
  );
}

function MasonryVirtualViewInner<T extends ClipboardItem>(
  {
    items,
    pinnedCount,
    cardDensity,
    cardMaxLines,
    imageMaxHeight,
    imageAutoHeight,
    scrollParent,
    renderCard,
  }: MasonryVirtualViewProps<T>,
  ref: React.ForwardedRef<MasonryVirtualViewHandle>,
) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [containerWidth, setContainerWidth] = useState(0);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(0);
  const [measureEpoch, setMeasureEpoch] = useState(0);

  const rowGapPx = masonryRowGapPx(cardDensity);
  const columnWidth = masonryColumnWidth(containerWidth);
  const settingsKey = `${cardMaxLines}-${imageMaxHeight}-${imageAutoHeight}-${cardDensity}-${columnWidth}`;

  useEffect(() => {
    const element = containerRef.current;
    if (!element) return;

    const readWidth = () => element.clientWidth;

    const observer = new ResizeObserver(() => {
      setContainerWidth(readWidth());
    });
    observer.observe(element);
    setContainerWidth(readWidth());
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    let ticking = false;
    const sync = () => {
      setScrollTop(scrollParent.scrollTop);
      setViewportHeight(scrollParent.clientHeight);
    };

    const onScroll = () => {
      if (ticking) return;
      ticking = true;
      requestAnimationFrame(() => {
        sync();
        ticking = false;
      });
    };

    sync();
    scrollParent.addEventListener("scroll", onScroll, { passive: true });
    const resizeObserver = new ResizeObserver(sync);
    resizeObserver.observe(scrollParent);
    return () => {
      scrollParent.removeEventListener("scroll", onScroll);
      resizeObserver.disconnect();
    };
  }, [scrollParent]);

  const resolveHeight = useCallback(
    (item: T) => {
      const cached = getCachedMasonryHeight(item.id, settingsKey);
      if (cached !== null) return cached;
      return estimateClipboardItemHeight(
        item,
        cardMaxLines,
        imageMaxHeight,
        imageAutoHeight,
        columnWidth,
      );
    },
    [settingsKey, cardMaxLines, imageMaxHeight, imageAutoHeight, columnWidth],
  );

  const handleHeightChange = useCallback(
    (itemId: number, height: number) => {
      if (setCachedMasonryHeight(itemId, height, settingsKey)) {
        setMeasureEpoch((epoch) => epoch + 1);
      }
    },
    [settingsKey],
  );

  const pinnedItems = useMemo(
    () => items.slice(0, pinnedCount),
    [items, pinnedCount],
  );
  const mainItems = useMemo(
    () => items.slice(pinnedCount),
    [items, pinnedCount],
  );

  const pinnedLayout = useMemo(
    () =>
      pinnedCount > 0
        ? buildMasonrySection(pinnedItems, 0, resolveHeight, rowGapPx)
        : null,
    [pinnedItems, pinnedCount, resolveHeight, rowGapPx, measureEpoch],
  );

  const mainLayout = useMemo(
    () => buildMasonrySection(mainItems, pinnedCount, resolveHeight, rowGapPx),
    [mainItems, pinnedCount, resolveHeight, rowGapPx, measureEpoch],
  );

  const pinnedBlockHeight = useMemo(() => {
    if (!pinnedLayout) return 0;
    return (
      pinnedLayout.totalHeight +
      (pinnedCount > 0 ? MASONRY_SEPARATOR_HEIGHT_PX : 0)
    );
  }, [pinnedLayout, pinnedCount]);

  const totalHeight = pinnedBlockHeight + mainLayout.totalHeight;

  const visiblePinned = useMemo(() => {
    if (!pinnedLayout) return [];
    return getVisiblePlacedItems(
      pinnedLayout.placed,
      scrollTop,
      viewportHeight,
      MASONRY_VIRTUAL_OVERSCAN_PX,
    );
  }, [pinnedLayout, scrollTop, viewportHeight]);

  const visibleMain = useMemo(() => {
    if (mainLayout.placed.length === 0) return [];
    const start = scrollTop - MASONRY_VIRTUAL_OVERSCAN_PX;
    const end = scrollTop + viewportHeight + MASONRY_VIRTUAL_OVERSCAN_PX;
    return mainLayout.placed.filter((entry) => {
      const absoluteTop = pinnedBlockHeight + entry.top;
      return absoluteTop + entry.height >= start && absoluteTop <= end;
    });
  }, [mainLayout, scrollTop, viewportHeight, pinnedBlockHeight]);

  useImperativeHandle(
    ref,
    () => ({
      scrollToIndex: (index: number, behavior: ScrollBehavior = "auto") => {
        const found = findPlacedItemByIndex(
          pinnedLayout,
          mainLayout,
          pinnedBlockHeight,
          index,
        );
        if (!found) return;
        scrollParent.scrollTo({
          top: Math.max(0, found.absoluteTop - 40),
          behavior,
        });
      },
    }),
    [pinnedLayout, mainLayout, pinnedBlockHeight, scrollParent],
  );

  return (
    <div style={{ height: totalHeight }}>
      <div
        ref={containerRef}
        className="relative pt-1.5 pb-2"
        style={{ minHeight: totalHeight }}
      >
        {visiblePinned.map((entry) =>
          renderPlacedItem(
            entry,
            columnWidth,
            0,
            handleHeightChange,
            renderCard,
          ),
        )}

        {pinnedLayout && pinnedCount > 0 && (
          <Separator
            className="absolute my-0"
            style={{
              top: pinnedLayout.totalHeight + 8,
              left: MASONRY_HORIZONTAL_INSET_PX,
              right: MASONRY_HORIZONTAL_INSET_PX,
            }}
          />
        )}

        {visibleMain.map((entry) =>
          renderPlacedItem(
            entry,
            columnWidth,
            pinnedBlockHeight,
            handleHeightChange,
            renderCard,
          ),
        )}
      </div>
    </div>
  );
}

export const MasonryVirtualView = forwardRef(MasonryVirtualViewInner) as <
  T extends ClipboardItem,
>(
  props: MasonryVirtualViewProps<T> & {
    ref?: React.ForwardedRef<MasonryVirtualViewHandle>;
  },
) => ReturnType<typeof MasonryVirtualViewInner>;
