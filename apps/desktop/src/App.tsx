import { useEffect, useState } from 'react';
import { Route, Routes } from 'react-router-dom';
import { AppLayout } from './components/layout/app-layout';
import { HomeScreen } from './screens/home-screen';
import { ContactListScreen } from './screens/contact-list-screen';
import { ContactDetailScreen } from './screens/contact-detail-screen';
import { ContactEditScreen } from './screens/contact-edit-screen';
import { SettingsScreen } from './screens/settings-screen';
import { ImportScreen } from './screens/import-screen';
import { DuplicatesScreen } from './screens/duplicates-screen';
import { TrashScreen } from './screens/trash-screen';
import { UnlockScreen } from './screens/unlock-screen';
import { securityStatus } from './lib/desktop-io';

export function App() {
  // `locked` is the one state that replaces the whole app: an encrypted
  // database whose key is not on this machine. Until the status answers —
  // or when it answers anything else — the app renders normally; in the
  // browser build the status is always 'browser'.
  const [locked, setLocked] = useState(false);
  useEffect(() => {
    void securityStatus().then((status) => setLocked(status.state === 'locked'));
  }, []);

  if (locked) {
    return <UnlockScreen />;
  }

  return (
    <Routes>
      <Route element={<AppLayout />}>
        <Route index element={<HomeScreen />} />
        <Route path="contacts" element={<ContactListScreen />} />
        <Route path="contacts/new" element={<ContactEditScreen mode="create" />} />
        <Route path="contacts/:id" element={<ContactDetailScreen />} />
        <Route path="contacts/:id/edit" element={<ContactEditScreen mode="edit" />} />
        <Route path="settings" element={<SettingsScreen />} />
        <Route path="import" element={<ImportScreen />} />
        <Route path="duplicates" element={<DuplicatesScreen />} />
        <Route path="trash" element={<TrashScreen />} />
      </Route>
    </Routes>
  );
}
