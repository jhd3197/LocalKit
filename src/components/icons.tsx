// UI icons — thin wrappers over lucide-react (plan 27), keeping the names and
// the `h-4 w-4` default the old hand-rolled set had so call sites don't care.
// Brand marks (WordPress, Docker, PHP…) are NOT here — those come from the
// curated offline Iconify subset in `lib/brandIcons.tsx`.
import type { LucideIcon } from "lucide-react";
import {
  ArrowUpDown,
  ArrowUpRight,
  Bookmark,
  Bug,
  Camera,
  Check,
  ChevronsLeft,
  ChevronsRight,
  Copy,
  Database,
  FileText,
  Globe,
  House,
  Keyboard,
  KeyRound,
  Layers,
  LayoutGrid,
  Link2,
  List,
  Play,
  Plus,
  Search,
  Server,
  Settings,
  SlidersHorizontal,
  Square,
  SquareTerminal,
  Trash2,
  Wrench,
  X,
} from "lucide-react";

interface IconProps {
  className?: string;
}

function wrap(Icon: LucideIcon) {
  return function WrappedIcon({ className }: IconProps) {
    return <Icon className={className ?? "h-4 w-4"} strokeWidth={1.75} aria-hidden />;
  };
}

export const GearIcon = wrap(Settings);
export const GridIcon = wrap(LayoutGrid);
export const ListIcon = wrap(List);
export const PlusIcon = wrap(Plus);
export const CloseIcon = wrap(X);
export const SlidersIcon = wrap(SlidersHorizontal);
export const TerminalIcon = wrap(SquareTerminal);
export const GlobeIcon = wrap(Globe);
export const ServerIcon = wrap(Server);
/** Chain link — marks a site imported from a ServerKit server (plan 18). */
export const LinkIcon = wrap(Link2);
export const KeyboardIcon = wrap(Keyboard);

// Actions (plan 27)
export const PlayIcon = wrap(Play);
export const StopSquareIcon = wrap(Square);
export const ArrowUpRightIcon = wrap(ArrowUpRight);
export const WrenchIcon = wrap(Wrench);
export const DuplicateIcon = wrap(Copy);
export const TrashIcon = wrap(Trash2);
export const BookmarkIcon = wrap(Bookmark);
export const CheckIcon = wrap(Check);

// Navigation + section headers (plans 27/28)
export const HomeIcon = wrap(House);
export const ChevronsLeftIcon = wrap(ChevronsLeft);
export const ChevronsRightIcon = wrap(ChevronsRight);
export const LayersIcon = wrap(Layers);
export const CameraIcon = wrap(Camera);
export const DatabaseIcon = wrap(Database);
export const KeyIcon = wrap(KeyRound);
export const FileTextIcon = wrap(FileText);
export const SyncIcon = wrap(ArrowUpDown);
export const SearchIcon = wrap(Search);
export const BugIcon = wrap(Bug);
