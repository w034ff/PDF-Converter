import { ContextMenu, type MenuButtonItem } from "../../components";
import { getTranslations } from "../../i18n";
import {
  keyOfPage,
  useAppDispatch,
  useAppState,
  type PageKey,
} from "../../state";

export interface OrganizePageMenuProps {
  x: number;
  y: number;
  /** The page pressed, which the selection holds. */
  pageKey: PageKey;
  /** Called with the deletion, which takes the page the focus would go back to. */
  onDelete: () => void;
  onClose: (returnFocus: boolean) => void;
}

/**
 * The menu of a page (organize §4). Turning and deleting work on the
 * selection, like the band; a break sits before one page, so that is the page
 * pressed.
 */
export function OrganizePageMenu({
  x,
  y,
  pageKey,
  onDelete,
  onClose,
}: OrganizePageMenuProps) {
  const { organize, language } = useAppState();
  const dispatch = useAppDispatch();
  const t = getTranslations(language.language).organize;
  const keys = [...organize.selected];

  const items: MenuButtonItem[] = [
    {
      id: "rotate-left",
      label: t.band.rotateLeft,
      shortcut: "L",
      onSelect: () =>
        dispatch({ type: "ORGANIZE_PAGES_ROTATED", keys, direction: "left" }),
    },
    {
      id: "rotate-right",
      label: t.band.rotateRight,
      shortcut: "R",
      onSelect: () =>
        dispatch({ type: "ORGANIZE_PAGES_ROTATED", keys, direction: "right" }),
    },
  ];
  if (organize.output === "split") {
    items.push({
      id: "break",
      label: organize.breaks.has(pageKey)
        ? t.pageMenu.removeBreak
        : t.pageMenu.addBreak,
      shortcut: "B",
      disabled: keyOfPage(organize.pages[0]) === pageKey,
      separatorBefore: true,
      onSelect: () =>
        dispatch({ type: "ORGANIZE_BREAK_TOGGLED", key: pageKey }),
    });
  }
  items.push({
    id: "delete",
    label: t.band.delete,
    shortcut: "Delete",
    separatorBefore: true,
    onSelect: () => {
      onDelete();
      dispatch({ type: "ORGANIZE_PAGES_DELETED", keys });
    },
  });

  return (
    <ContextMenu
      x={x}
      y={y}
      label={t.pageMenu.label}
      items={items}
      onClose={onClose}
    />
  );
}
