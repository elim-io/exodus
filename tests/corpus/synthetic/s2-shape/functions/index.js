const { onRequest } = require('firebase-functions/v2/https');
const { onSchedule } = require('firebase-functions/v2/scheduler');
const { initializeApp } = require('firebase-admin/app');
const { getFirestore, FieldValue } = require('firebase-admin/firestore');

initializeApp();
const db = getFirestore();

exports.signup = onRequest({ region: 'europe-west1', cors: true }, async (req, res) => {
  const email = String(req.body.email || '').trim().toLowerCase();
  if (!email.includes('@')) {
    res.status(400).send('bad email');
    return;
  }
  await db.collection('signups').doc(email).set({ at: FieldValue.serverTimestamp() });
  res.status(204).end();
});

exports.dailyCount = onSchedule({ schedule: 'every day 06:00', region: 'europe-west1' }, async () => {
  const snap = await db.collection('signups').count().get();
  await db.collection('stats').doc('signups').set({ total: snap.data().count, at: FieldValue.serverTimestamp() });
});
