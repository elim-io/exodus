import { Worker } from 'bullmq';
import { redis } from './queue.js';
import { pool } from './db.js';

async function handle(job) {
  await pool.query('insert into job_log (name, data) values ($1, $2)', [job.name, job.data]);
}

new Worker('jobs', handle, { connection: redis });
