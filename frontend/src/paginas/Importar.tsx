import { Alert, Anchor, Badge, Breadcrumbs, Button, Card, Checkbox, Group, Loader, Stack, Text, Title } from '@mantine/core'
import { useMutation, useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { Link } from 'react-router'

import { api } from '../api'
import { n, resumenPerfil } from '../formato'
import { useRefrescarTaller } from '../opciones'
import type { Importables, Propuesta } from '../tipos'

const TITULOS: Record<Propuesta['clase'], string> = {
  impresora: 'Impresoras',
  material: 'Materiales',
  perfil: 'Perfiles',
}

const RUTA: Record<Propuesta['clase'], (id: number) => string> = {
  impresora: (id) => `/impresoras/${id}`,
  material: () => '/impresoras?ver=materiales',
  perfil: (id) => `/perfiles/${id}`,
}

function Fila({
  p,
  marcada,
  necesaria,
  onChange,
  nombreDe,
}: {
  p: Propuesta
  marcada: boolean
  necesaria: boolean
  onChange: (v: boolean) => void
  nombreDe: (clave: string | null) => string
}) {
  const ya = p.ya !== null
  return (
    <Group align="flex-start" wrap="nowrap" gap="sm" py={6} style={{ borderTop: '1px solid var(--mantine-color-default-border)' }}>
      <Checkbox
        mt={3}
        checked={ya || marcada || necesaria}
        disabled={ya || !p.importable || necesaria}
        onChange={(e) => onChange(e.currentTarget.checked)}
        aria-label={`Importar ${p.nombre}`}
      />
      <Stack gap={2} style={{ flex: 1, minWidth: 0 }}>
        <Group gap={6}>
          <Text size="sm" fw={500}>
            {p.nombre}
          </Text>
          {ya && (
            <Badge size="xs" color="green" variant="light" component={Link} to={RUTA[p.clase](p.ya!)} style={{ cursor: 'pointer' }}>
              ya está
            </Badge>
          )}
          {necesaria && !ya && !marcada && (
            <Badge size="xs" color="blue" variant="outline">
              entra con sus perfiles
            </Badge>
          )}
          {p.predeterminado && (
            <Badge size="xs" color="yellow" variant="light">
              activo en el laminador
            </Badge>
          )}
        </Group>
        <Text size="xs" c="dimmed">
          {p.detalle}
          {p.clase === 'perfil' && p.material ? ` · ${nombreDe(p.material)}` : ''}
        </Text>
        {p.clase === 'perfil' && p.datos.parametros && Object.keys(p.datos.parametros).length > 0 && (
          <Text size="xs">{resumenPerfil({ capa_mm: p.datos.capa_mm ?? null, parametros: p.datos.parametros })}</Text>
        )}
        {p.avisos.map((a) => (
          <Text key={a} size="xs" c="orange">
            {a}
          </Text>
        ))}
        <Text size="xs" c="dimmed" style={{ wordBreak: 'break-all' }}>
          {p.fuentes.length === 1 ? p.fuentes[0] : `${p.fuentes[0]} y ${p.fuentes.length - 1} más`}
          {p.archivos.length > 0 && ` · liga ${p.archivos.length} ${p.archivos.length === 1 ? 'archivo' : 'archivos'}`}
        </Text>
      </Stack>
    </Group>
  )
}

export default function Importar() {
  const refrescar = useRefrescarTaller()
  const q = useQuery({ queryKey: ['importables'], queryFn: () => api<Importables>('/taller/importar') })
  const [cambios, setCambios] = useState<Record<string, boolean>>({})
  const importar = useMutation({
    mutationFn: (claves: string[]) =>
      api<{ impresoras: number; materiales: number; perfiles: number; archivos: number }>('/taller/importar', {
        method: 'POST',
        body: JSON.stringify({ claves }),
      }),
    onSuccess: () => {
      setCambios({})
      refrescar()
    },
  })
  if (q.isPending) return <Loader />
  if (q.error) return <Alert color="red">{q.error.message}</Alert>
  const { propuestas, errores } = q.data
  const marcada = (p: Propuesta) => p.ya === null && p.importable && (cambios[p.clave] ?? p.marcada)
  const elegidas = propuestas.filter(marcada)
  // las impresoras y materiales que piden los perfiles marcados entran solos
  const necesarias = new Set(
    elegidas.filter((p) => p.clase === 'perfil').flatMap((p) => [p.impresora, p.material].filter((c): c is string => !!c)),
  )
  const total = new Set([...elegidas.map((p) => p.clave), ...[...necesarias].filter((c) => propuestas.some((p) => p.clave === c && p.ya === null))])
  const nombreDe = (clave: string | null) => propuestas.find((p) => p.clave === clave)?.nombre ?? ''

  return (
    <Stack maw={900}>
      <Breadcrumbs>
        <Anchor component={Link} to="/impresoras">
          Impresoras y materiales
        </Anchor>
        <Text>Importar</Text>
      </Breadcrumbs>
      <div>
        <Title order={2}>Importar de tus archivos de perfiles</Title>
        <Text size="sm" c="dimmed">
          Lo que encontré en las escenas y perfiles de Photon Workshop, los paquetes de Orca y los perfiles de Lychee de la
          colección. No se mueve ningún archivo: solo se guardan sus datos y se ligan los archivos a sus perfiles. Después
          se puede editar todo.
        </Text>
      </div>
      {importar.data && (
        <Alert color="green" title="Listo">
          Se importaron {n(importar.data.impresoras)} impresoras, {n(importar.data.materiales)} materiales y{' '}
          {n(importar.data.perfiles)} perfiles (con {n(importar.data.archivos)} archivos ligados).
        </Alert>
      )}
      {importar.error && <Alert color="red">{importar.error.message}</Alert>}
      {(['impresora', 'material', 'perfil'] as const).map((clase) => {
        const lista = propuestas.filter((p) => p.clase === clase)
        if (lista.length === 0) return null
        return (
          <Card withBorder key={clase} padding="sm">
            <Text fw={600} mb={4}>
              {TITULOS[clase]}
            </Text>
            {lista.map((p) => (
              <Fila
                key={p.clave}
                p={p}
                marcada={marcada(p)}
                necesaria={necesarias.has(p.clave)}
                nombreDe={nombreDe}
                onChange={(v) => setCambios({ ...cambios, [p.clave]: v })}
              />
            ))}
          </Card>
        )
      })}
      {propuestas.length === 0 && <Text c="dimmed">No encontré archivos de perfiles en la colección.</Text>}
      {errores.length > 0 && (
        <Alert color="orange" title="Archivos que no se pudieron leer">
          {errores.map((e) => (
            <Text key={e} size="sm" style={{ wordBreak: 'break-all' }}>
              {e}
            </Text>
          ))}
        </Alert>
      )}
      <Group>
        <Button disabled={total.size === 0} loading={importar.isPending} onClick={() => importar.mutate([...total])}>
          Importar {n(total.size)}
        </Button>
        <Button variant="subtle" component={Link} to="/impresoras">
          Volver
        </Button>
      </Group>
    </Stack>
  )
}
