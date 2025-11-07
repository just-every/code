[Compaction Summary]

Recent exchanges (newest last):
{% if snippets.len() == 0 %}- (no user or assistant messages recorded)
{% else %}{% for snippet in snippets %}- ({{ snippet.role }}) {{ snippet.text }}
{% endfor %}{% endif %}

Key takeaways:
{{ summary_text }}
