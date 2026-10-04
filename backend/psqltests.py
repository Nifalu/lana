import psycopg

with psycopg.connect(
    host="localhost",
    port=5432,
    dbname="hackamrhein",
    user="postgres",
    password="hackzheworld",
) as conn:
    print(conn.execute("SELECT version()").fetchone())
