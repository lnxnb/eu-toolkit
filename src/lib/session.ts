export interface Session {
  installPath: string;
  /** Mod project folder overlaying the base game; null = base game only. */
  modPath: string | null;
  projectName: string | null;
  /**
   * The "view/edit at" date (Sprint 12.2), "Y.M.D". null = the effective start
   * (default bookmark, else earliest, else 1444.11.11), resolved server-side per
   * command. Persisted per project in the settings DB (`selected_date:<scope>`);
   * MapView owns the live value and loads/saves it there.
   */
  selectedDate?: string | null;
}
