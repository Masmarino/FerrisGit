{{- define "ferrisgit-website.labels" -}}
app.kubernetes.io/name: {{ .Chart.Name }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/component: web
app.kubernetes.io/version: {{ toString (.Values.image.tag | default .Chart.AppVersion) | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" }}
{{- end -}}

{{- define "ferrisgit-website.selectorLabels" -}}
app.kubernetes.io/name: {{ .Chart.Name }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{- define "ferrisgit-website.image" -}}
{{- $tag := required "image.tag must be set to a release tag (e.g. --set image.tag=0.2.0)" (toString (.Values.image.tag | default "")) -}}
{{- if .Values.image.digest -}}
{{ .Values.image.repository }}:{{ $tag }}@{{ .Values.image.digest }}
{{- else -}}
{{ .Values.image.repository }}:{{ $tag }}
{{- end -}}
{{- end -}}

{{/* A moving tag would never be refreshed under IfNotPresent. */}}
{{- define "ferrisgit-website.pullPolicy" -}}
{{- if and (eq (toString .Values.image.tag) "latest") (not .Values.image.digest) }}Always{{ else }}{{ .Values.image.pullPolicy }}{{ end -}}
{{- end -}}

{{- define "ferrisgit-website.tlsSecretName" -}}
{{ .Values.ingress.tlsSecretName | default (printf "%s-tls" .Release.Name) }}
{{- end -}}
