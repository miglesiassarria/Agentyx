type AppView = 'workspace' | 'settings';

class UiStore {
  activeView = $state<AppView>('workspace');

  showWorkspace(): void {
    this.activeView = 'workspace';
  }

  showSettings(): void {
    this.activeView = 'settings';
  }
}

export const uiStore = new UiStore();
