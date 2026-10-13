import express from 'express';
import session from 'express-session';
import multer from 'multer';
import path from 'node:path';
import fs from 'node:fs/promises';
import { pool } from './db.js';
import { jobs } from './queue.js';

const app = express();
const UPLOAD_DIR = process.env.UPLOAD_DIR || 'uploads';
const upload = multer({ storage: multer.memoryStorage() });

app.use(express.json());
app.use(session({ secret: process.env.SESSION_SECRET, resave: false, saveUninitialized: false }));

app.get('/healthz', async (_req, res) => {
  await pool.query('select 1');
  res.send('ok');
});

app.post('/login', async (req, res) => {
  const { rows } = await pool.query('select id from users where email = $1', [req.body.email]);
  if (!rows.length) return res.status(401).end();
  req.session.userId = rows[0].id;
  res.end();
});

app.post('/upload', upload.single('file'), async (req, res) => {
  const name = `${Date.now()}-${req.file.originalname}`;
  await fs.writeFile(path.join(UPLOAD_DIR, name), req.file.buffer);
  await jobs.add('thumbnail', { name });
  res.json({ name });
});

app.listen(process.env.PORT || 3000);
